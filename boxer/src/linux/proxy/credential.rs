use crate::{
    credential,
    network::{Target, UpstreamProxy},
    policy::{CredentialInjectionMode, CredentialProvider, CustomCredential, EndpointRule},
};
use base64::Engine;
use ring::rand::{SecureRandom, SystemRandom};
use rustls::{ClientConfig, RootCertStore, pki_types::ServerName};
use std::{
    collections::{BTreeMap, HashMap},
    io,
    net::TcpListener,
    sync::Arc,
    thread::{self, JoinHandle},
    time::Duration,
};
use subtle::ConstantTimeEq;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::oneshot,
};
use tokio_rustls::TlsConnector;
use zeroize::{Zeroize, Zeroizing};

const MAX_HEADER: usize = 16_384;
const MAX_CONTENT_LENGTH: u64 = 32 * 1024 * 1024;
const HEADER_TIMEOUT: Duration = Duration::from_secs(15);

struct Credential {
    name: String,
    authority: String,
    host: String,
    incoming_header: String,
    credential_format: String,
    inject_mode: CredentialInjectionMode,
    path_pattern: Option<String>,
    path_replacement: Option<String>,
    query_param_name: Option<String>,
    secret: Zeroizing<String>,
    token: Zeroizing<String>,
}

pub struct BrokeredCredential {
    pub name: String,
    pub token_env: String,
    pub base_env: String,
    pub base_path: String,
    pub token: String,
}

pub struct Broker {
    port: u16,
    shutdown: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<io::Result<()>>>,
}

pub struct BrokerOptions<'a> {
    pub endpoint_rules: &'a [EndpointRule],
    pub upstream_proxy: Option<&'a UpstreamProxy>,
    pub upstream_bypass: &'a [String],
    pub denied_hosts: &'a [String],
    pub reserved_ports: &'a [u16],
}

impl Broker {
    pub fn start(
        providers: &[String],
        custom_credentials: &BTreeMap<String, CustomCredential>,
        options: BrokerOptions<'_>,
    ) -> io::Result<(Self, Vec<BrokeredCredential>)> {
        let BrokerOptions {
            endpoint_rules,
            upstream_proxy,
            upstream_bypass,
            denied_hosts,
            reserved_ports,
        } = options;
        let mut credentials = HashMap::new();
        let mut session_tokens = Vec::new();
        for name in providers {
            let (
                credential_key,
                token_env,
                base_path,
                authority,
                host,
                incoming_header,
                format,
                inject_mode,
                path_pattern,
                path_replacement,
                query_param_name,
            ) = if let Some(custom) = custom_credentials.get(name) {
                let url = url::Url::parse(&custom.upstream).map_err(io::Error::other)?;
                let host = url
                    .host_str()
                    .ok_or_else(|| io::Error::other("Custom upstream host is missing"))?;
                let port = url.port_or_known_default().unwrap_or(443);
                let authority = if port == 443 {
                    format!("{host}:443")
                } else {
                    format!("{host}:{port}")
                };
                (
                    custom.credential_key.clone(),
                    custom.token_env(name),
                    url.path().trim_end_matches('/').to_owned(),
                    authority,
                    host.to_owned(),
                    custom.inject_header.clone(),
                    custom.credential_format.clone(),
                    custom.inject_mode,
                    custom.path_pattern.clone(),
                    custom.path_replacement.clone(),
                    custom.query_param_name.clone(),
                )
            } else if let Ok(provider) = CredentialProvider::parse(name) {
                let (path, header, format) = builtin_route(provider);
                (
                    provider.key_env().to_owned(),
                    provider.key_env().to_owned(),
                    path.to_owned(),
                    provider.host().to_owned(),
                    provider.host().trim_end_matches(":443").to_owned(),
                    header.to_owned(),
                    format.to_owned(),
                    CredentialInjectionMode::Header,
                    None,
                    None,
                    None,
                )
            } else {
                return Err(io::Error::other(format!(
                    "Custom credential route {name} is not defined"
                )));
            };
            if crate::network::is_denied_domain(&authority, denied_hosts) {
                return Err(io::Error::other(format!(
                    "Credential route {name} uses a domain denied by the network policy"
                )));
            }
            let (_, secret) = credential::load(std::slice::from_ref(&credential_key))?
                .pop()
                .ok_or_else(|| io::Error::other("Credential store returned no value"))?;
            if secret.bytes().any(|byte| byte.is_ascii_control()) {
                return Err(io::Error::other(format!(
                    "Credential {credential_key} contains unsupported control characters"
                )));
            }
            if inject_mode == CredentialInjectionMode::BasicAuth && !secret.contains(':') {
                return Err(io::Error::other(format!(
                    "Credential {credential_key} for basic_auth must be stored as username:password"
                )));
            }
            let token = session_token()?;
            let entry = Credential {
                name: name.clone(),
                authority,
                host,
                incoming_header,
                credential_format: format,
                inject_mode,
                path_pattern,
                path_replacement,
                query_param_name,
                secret,
                token: Zeroizing::new(token.clone()),
            };
            credentials.insert(name.clone(), Arc::new(entry));
            let local_path = if base_path.is_empty() {
                format!("/{name}/")
            } else {
                format!("/{name}{base_path}/")
            };
            session_tokens.push(BrokeredCredential {
                name: name.clone(),
                token_env,
                base_env: if custom_credentials.contains_key(name) {
                    CustomCredential::base_env(name)
                } else if CredentialProvider::parse(name).is_ok() {
                    builtin_base_env(CredentialProvider::parse(name).unwrap()).to_owned()
                } else {
                    CustomCredential::base_env(name)
                },
                base_path: local_path,
                token,
            });
        }
        let endpoint_rules = Arc::new(endpoint_rules.to_vec());
        let upstream_proxy = upstream_proxy.cloned();
        let upstream_bypass = Arc::new(upstream_bypass.to_vec());

        let listener = bind_listener(reserved_ports)?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let (shutdown, mut stop) = oneshot::channel();
        let (ready, started) = std::sync::mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("boxer-credential-proxy".into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()?;
                runtime.block_on(async move {
                    let listener = match tokio::net::TcpListener::from_std(listener) {
                        Ok(listener) => {
                            let _ = ready.send(Ok(()));
                            listener
                        }
                        Err(error) => {
                            let _ =
                                ready.send(Err(io::Error::new(error.kind(), error.to_string())));
                            return Err(error);
                        }
                    };
                    loop {
                        tokio::select! {
                            _ = &mut stop => break,
                            accepted = listener.accept() => {
                                let (stream, _) = accepted?;
                                let credentials = credentials.clone();
                                let endpoint_rules = endpoint_rules.clone();
                                let upstream_proxy = upstream_proxy.clone();
                                let upstream_bypass = upstream_bypass.clone();
                                tokio::spawn(async move {
                                    let _ = serve(
                                        stream,
                                        credentials,
                                        endpoint_rules,
                                        upstream_proxy,
                                        upstream_bypass,
                                    )
                                    .await;
                                });
                            }
                        }
                    }
                    Ok(())
                })
            })?;
        started
            .recv()
            .map_err(|_| io::Error::other("Credential proxy failed to start"))??;
        Ok((
            Self {
                port,
                shutdown: Some(shutdown),
                thread: Some(thread),
            },
            session_tokens,
        ))
    }

    pub fn port(&self) -> u16 {
        self.port
    }
}

fn bind_listener(reserved_ports: &[u16]) -> io::Result<TcpListener> {
    for _ in 0..64 {
        let candidate = TcpListener::bind("127.0.0.1:0")?;
        if !reserved_ports.contains(&candidate.local_addr()?.port()) {
            return Ok(candidate);
        }
    }
    Err(io::Error::other(
        "Could not allocate a free credential proxy port",
    ))
}

impl Drop for Broker {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn session_token() -> io::Result<String> {
    let mut bytes = [0; 32];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| io::Error::other("Could not create a credential proxy session"))?;
    Ok(hex::encode(bytes))
}

fn builtin_route(provider: CredentialProvider) -> (&'static str, &'static str, &'static str) {
    match provider {
        CredentialProvider::Openai => ("/v1", "Authorization", "Bearer {}"),
        CredentialProvider::Anthropic => ("", "x-api-key", "{}"),
        CredentialProvider::Gemini => ("", "x-goog-api-key", "{}"),
        CredentialProvider::Github => ("", "Authorization", "token {}"),
        CredentialProvider::Gitlab => ("/api", "Authorization", "Bearer {}"),
    }
}

fn builtin_base_env(provider: CredentialProvider) -> &'static str {
    match provider {
        CredentialProvider::Openai => "OPENAI_BASE_URL",
        CredentialProvider::Anthropic => "ANTHROPIC_BASE_URL",
        CredentialProvider::Gemini => "GEMINI_BASE_URL",
        CredentialProvider::Github => "GITHUB_API_URL",
        CredentialProvider::Gitlab => "GITLAB_API_URL",
    }
}

async fn serve(
    mut client: TcpStream,
    credentials: HashMap<String, Arc<Credential>>,
    endpoint_rules: Arc<Vec<EndpointRule>>,
    upstream_proxy: Option<UpstreamProxy>,
    upstream_bypass: Arc<Vec<String>>,
) -> io::Result<()> {
    #[cfg(target_os = "macos")]
    let _ = (&upstream_proxy, &upstream_bypass);
    client.set_nodelay(true)?;
    let header =
        match tokio::time::timeout(HEADER_TIMEOUT, read_header(&mut client, MAX_HEADER)).await {
            Err(_) => return response(&mut client, 408, "Request header timed out").await,
            Ok(Ok(header)) => header,
            Ok(Err(_)) => return response(&mut client, 400, "Invalid request").await,
        };
    let request = match parse_request(&header, &credentials, &endpoint_rules) {
        Ok(request) => request,
        Err(RequestError::Unauthorized) => {
            return response(&mut client, 407, "Credential proxy authentication required").await;
        }
        Err(RequestError::Forbidden) => {
            return response(&mut client, 403, "Endpoint is not allowed by Boxer policy").await;
        }
        Err(RequestError::Invalid) => return response(&mut client, 400, "Invalid request").await,
    };
    if request.expect_continue {
        client.write_all(b"HTTP/1.1 100 Continue\r\n\r\n").await?;
    }
    let target = Target::parse(&request.credential.authority, false)?;
    #[cfg(target_os = "linux")]
    let connected =
        super::host::connect_route(&target, false, upstream_proxy.as_ref(), &upstream_bypass).await;
    #[cfg(target_os = "macos")]
    let connected = connect_provider(&target).await;
    let socket = match connected {
        Ok(socket) => socket,
        Err(_) => return response(&mut client, 502, "Provider connection failed").await,
    };
    socket.set_nonblocking(true)?;
    let socket = TcpStream::from_std(socket)?;
    let connector = match tls_connector() {
        Ok(connector) => connector,
        Err(_) => return response(&mut client, 502, "Provider connection failed").await,
    };
    let tls = match connector.connect(request.server_name, socket).await {
        Ok(tls) => tls,
        Err(_) => return response(&mut client, 502, "Provider connection failed").await,
    };
    let (mut remote_reader, mut remote_writer) = tokio::io::split(tls);
    remote_writer
        .write_all(request.upstream_header.as_bytes())
        .await?;
    let (client_reader, mut client_writer) = tokio::io::split(client);

    let upload = async {
        let mut limited = client_reader.take(request.body_limit);
        let result = tokio::io::copy(&mut limited, &mut remote_writer).await;
        let _ = remote_writer.shutdown().await;
        result.map(|_| ())
    };
    let download = async {
        let response_header = read_header(&mut remote_reader, MAX_HEADER).await?;
        let response_header = close_response_connection(&response_header)?;
        client_writer.write_all(&response_header).await?;
        client_writer.flush().await?;
        tokio::io::copy(&mut remote_reader, &mut client_writer).await?;
        let _ = client_writer.shutdown().await;
        Ok::<_, io::Error>(())
    };
    let _ = tokio::join!(upload, download);
    Ok(())
}

#[cfg(target_os = "macos")]
async fn connect_provider(target: &Target) -> io::Result<std::net::TcpStream> {
    let addresses = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::net::lookup_host((target.host.as_str(), target.port))
            .await
            .map(|addresses| addresses.take(32).collect::<Vec<_>>())
    })
    .await
    .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "Provider DNS lookup timed out"))??;
    if addresses.is_empty()
        || addresses
            .iter()
            .any(|address| !crate::network::is_globally_routable(address.ip()))
    {
        return Err(io::Error::other(
            "Provider route cannot resolve to private or special-use addresses",
        ));
    }
    let mut failure = io::Error::other("No provider address was reachable");
    for address in addresses {
        match tokio::time::timeout(
            Duration::from_millis(750),
            tokio::net::TcpStream::connect(address),
        )
        .await
        .unwrap_or_else(|_| {
            Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Provider timed out",
            ))
        }) {
            Ok(stream) => {
                let stream = stream.into_std()?;
                stream.set_nonblocking(false)?;
                return Ok(stream);
            }
            Err(error) => failure = error,
        }
    }
    Err(failure)
}

struct Request {
    credential: Arc<Credential>,
    server_name: ServerName<'static>,
    upstream_header: Zeroizing<String>,
    body_limit: u64,
    expect_continue: bool,
}

#[derive(Debug)]
enum RequestError {
    Invalid,
    Unauthorized,
    Forbidden,
}

fn parse_request(
    header: &[u8],
    credentials: &HashMap<String, Arc<Credential>>,
    endpoint_rules: &[EndpointRule],
) -> Result<Request, RequestError> {
    let text = std::str::from_utf8(header).map_err(|_| RequestError::Invalid)?;
    let mut lines = text.split("\r\n");
    let mut request_line = lines.next().ok_or(RequestError::Invalid)?.split(' ');
    let method = request_line.next().ok_or(RequestError::Invalid)?;
    let path = request_line.next().ok_or(RequestError::Invalid)?;
    let version = request_line.next().ok_or(RequestError::Invalid)?;
    if request_line.next().is_some()
        || !valid_http_token(method)
        || version != "HTTP/1.1"
        || !path.starts_with('/')
        || path
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte == b' ')
    {
        return Err(RequestError::Invalid);
    }
    let (route, upstream_path) = path[1..].split_once('/').ok_or(RequestError::Invalid)?;
    let credential = credentials
        .get(route)
        .cloned()
        .ok_or(RequestError::Invalid)?;
    let mut upstream_path = Zeroizing::new(format!("/{upstream_path}"));
    let mut headers = Vec::new();
    let mut supplied_token = None;
    let mut content_length = None;
    let mut chunked = false;
    let mut expect_continue = false;
    for line in lines.take_while(|line| !line.is_empty()) {
        let (name, value) = line.split_once(':').ok_or(RequestError::Invalid)?;
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte))
            || value
                .bytes()
                .any(|byte| (byte.is_ascii_control() && byte != b'\t') || byte == 127)
        {
            return Err(RequestError::Invalid);
        }
        let value = value.trim();
        if name.eq_ignore_ascii_case("content-length") {
            let length = value.parse::<u64>().map_err(|_| RequestError::Invalid)?;
            if length > MAX_CONTENT_LENGTH {
                return Err(RequestError::Invalid);
            }
            if content_length.replace(length).is_some() {
                return Err(RequestError::Invalid);
            }
            headers.push((name.to_owned(), value.to_owned()));
        } else if name.eq_ignore_ascii_case("transfer-encoding") {
            if chunked || !value.eq_ignore_ascii_case("chunked") {
                return Err(RequestError::Invalid);
            }
            chunked = true;
            headers.push((name.to_owned(), value.to_owned()));
        } else if name.eq_ignore_ascii_case("expect") {
            if expect_continue || !value.eq_ignore_ascii_case("100-continue") {
                return Err(RequestError::Invalid);
            }
            expect_continue = true;
        } else if matches!(
            credential.inject_mode,
            CredentialInjectionMode::Header | CredentialInjectionMode::BasicAuth
        ) && name.eq_ignore_ascii_case(&credential.incoming_header)
        {
            if supplied_token.replace(value.to_owned()).is_some() {
                return Err(RequestError::Unauthorized);
            }
        } else if [
            "authorization",
            "x-api-key",
            "x-goog-api-key",
            "anthropic-auth-token",
        ]
        .iter()
        .any(|blocked| name.eq_ignore_ascii_case(blocked))
        {
            // Caller-supplied credentials cannot override a brokered route.
        } else if ![
            "host",
            "connection",
            "proxy-connection",
            "proxy-authorization",
            "expect",
        ]
        .iter()
        .any(|skip| name.eq_ignore_ascii_case(skip))
        {
            headers.push((name.to_owned(), value.to_owned()));
        }
    }
    if chunked && content_length.is_some() {
        return Err(RequestError::Invalid);
    }
    let mut supplied_token = match credential.inject_mode {
        CredentialInjectionMode::Header => {
            let value = supplied_token.ok_or(RequestError::Unauthorized)?;
            Zeroizing::new(
                extract_formatted_token(&credential.credential_format, &value)
                    .ok_or(RequestError::Unauthorized)?
                    .as_bytes()
                    .to_vec(),
            )
        }
        CredentialInjectionMode::BasicAuth => {
            let value = supplied_token.ok_or(RequestError::Unauthorized)?;
            let encoded = value
                .strip_prefix("Basic ")
                .ok_or(RequestError::Unauthorized)?;
            Zeroizing::new(
                base64::engine::general_purpose::STANDARD
                    .decode(encoded)
                    .map_err(|_| RequestError::Unauthorized)?,
            )
        }
        CredentialInjectionMode::UrlPath => {
            let pattern = credential
                .path_pattern
                .as_deref()
                .ok_or(RequestError::Invalid)?;
            Zeroizing::new(
                extract_path_credential(upstream_path.as_str(), pattern)
                    .ok_or(RequestError::Unauthorized)?
                    .as_bytes()
                    .to_vec(),
            )
        }
        CredentialInjectionMode::QueryParam => {
            let parameter = credential
                .query_param_name
                .as_deref()
                .ok_or(RequestError::Invalid)?;
            let token = query_credential(upstream_path.as_str(), parameter, None)?
                .ok_or(RequestError::Unauthorized)?;
            Zeroizing::new(token.as_bytes().to_vec())
        }
    };
    if !bool::from(supplied_token.as_slice().ct_eq(credential.token.as_bytes())) {
        return Err(RequestError::Unauthorized);
    }
    match credential.inject_mode {
        CredentialInjectionMode::UrlPath => {
            replace_path_credential(
                &mut upstream_path,
                credential
                    .path_pattern
                    .as_deref()
                    .ok_or(RequestError::Invalid)?,
                credential
                    .path_replacement
                    .as_deref()
                    .or(credential.path_pattern.as_deref())
                    .ok_or(RequestError::Invalid)?,
                &credential.secret,
            )?;
        }
        CredentialInjectionMode::QueryParam => {
            upstream_path = query_credential(
                upstream_path.as_str(),
                credential
                    .query_param_name
                    .as_deref()
                    .ok_or(RequestError::Invalid)?,
                Some(credential.secret.as_str()),
            )?
            .ok_or(RequestError::Unauthorized)?;
        }
        CredentialInjectionMode::Header | CredentialInjectionMode::BasicAuth => {}
    }
    supplied_token.zeroize();
    if !endpoint_rules.is_empty() {
        let path = upstream_path
            .split_once('?')
            .map(|(path, _)| path)
            .unwrap_or(upstream_path.as_str());
        let path = decode_path(path).ok_or(RequestError::Invalid)?;
        if !endpoint_rules
            .iter()
            .any(|rule| rule.matches(&credential.name, method, &path))
        {
            return Err(RequestError::Forbidden);
        }
    }

    let host = credential.authority.trim_end_matches(":443");
    let mut upstream_header = format!(
        "{method} {} {version}\r\nHost: {host}\r\n",
        upstream_path.as_str()
    );
    for (name, value) in headers {
        upstream_header.push_str(&name);
        upstream_header.push_str(": ");
        upstream_header.push_str(&value);
        upstream_header.push_str("\r\n");
    }
    match credential.inject_mode {
        CredentialInjectionMode::Header => {
            let (prefix, suffix) = credential.credential_format.split_once("{}").unwrap();
            let mut injected = Zeroizing::new(String::with_capacity(
                prefix.len() + credential.secret.len() + suffix.len(),
            ));
            injected.push_str(prefix);
            injected.push_str(&credential.secret);
            injected.push_str(suffix);
            upstream_header.push_str(&credential.incoming_header);
            upstream_header.push_str(": ");
            upstream_header.push_str(&injected);
            upstream_header.push_str("\r\n");
        }
        CredentialInjectionMode::BasicAuth => {
            let mut encoded = Zeroizing::new(
                base64::engine::general_purpose::STANDARD.encode(credential.secret.as_bytes()),
            );
            upstream_header.push_str(&credential.incoming_header);
            upstream_header.push_str(": Basic ");
            upstream_header.push_str(&encoded);
            upstream_header.push_str("\r\n");
            encoded.zeroize();
        }
        CredentialInjectionMode::UrlPath | CredentialInjectionMode::QueryParam => {}
    }
    upstream_header.push_str("Connection: close\r\n\r\n");
    let server_name =
        ServerName::try_from(credential.host.clone()).map_err(|_| RequestError::Invalid)?;
    let body_limit = content_length.unwrap_or(if chunked { MAX_CONTENT_LENGTH } else { 0 });
    Ok(Request {
        credential,
        server_name,
        upstream_header: Zeroizing::new(upstream_header),
        body_limit,
        expect_continue,
    })
}

fn extract_formatted_token<'a>(format: &str, value: &'a str) -> Option<&'a str> {
    let (prefix, suffix) = format.split_once("{}")?;
    let value_prefix = value.get(..prefix.len())?;
    if !value_prefix.eq_ignore_ascii_case(prefix) {
        return None;
    }
    let token = value.get(prefix.len()..)?.strip_suffix(suffix)?;
    (!token.is_empty()).then_some(token)
}

fn extract_path_credential<'a>(path: &'a str, pattern: &str) -> Option<&'a str> {
    let path = path.split_once('?').map_or(path, |(path, _)| path);
    let (prefix, suffix) = pattern.split_once("{}")?;
    let after_prefix = path.strip_prefix(prefix)?;
    let token_end = if suffix.is_empty() {
        after_prefix.len()
    } else {
        after_prefix.find(suffix)?
    };
    (token_end > 0).then_some(&after_prefix[..token_end])
}

fn replace_path_credential(
    path: &mut Zeroizing<String>,
    pattern: &str,
    replacement: &str,
    secret: &str,
) -> Result<(), RequestError> {
    let (path_only, query) = path
        .as_str()
        .split_once('?')
        .map_or((path.as_str(), None), |(path, query)| (path, Some(query)));
    let (pattern_prefix, pattern_suffix) = pattern.split_once("{}").ok_or(RequestError::Invalid)?;
    let (replacement_prefix, replacement_suffix) =
        replacement.split_once("{}").ok_or(RequestError::Invalid)?;
    let after_prefix = path_only
        .strip_prefix(pattern_prefix)
        .ok_or(RequestError::Unauthorized)?;
    let token_end = if pattern_suffix.is_empty() {
        after_prefix.len()
    } else {
        after_prefix
            .find(pattern_suffix)
            .ok_or(RequestError::Unauthorized)?
    };
    if token_end == 0 {
        return Err(RequestError::Unauthorized);
    }
    let tail_start = token_end + pattern_suffix.len();
    let encoded = Zeroizing::new(encode_path_segment(secret));
    let mut rewritten = Zeroizing::new(String::with_capacity(path.len() + encoded.len()));
    rewritten.push_str(replacement_prefix);
    rewritten.push_str(&encoded);
    rewritten.push_str(replacement_suffix);
    rewritten.push_str(&after_prefix[tail_start..]);
    if let Some(query) = query {
        rewritten.push('?');
        rewritten.push_str(query);
    }
    *path = rewritten;
    Ok(())
}

fn query_credential(
    target: &str,
    parameter: &str,
    replacement: Option<&str>,
) -> Result<Option<Zeroizing<String>>, RequestError> {
    let (path, query) = target.split_once('?').ok_or(RequestError::Unauthorized)?;
    let mut found = None;
    let mut matches = 0;
    let mut rewritten = Vec::new();
    for item in query.split('&') {
        let mut pair = form_urlencoded::parse(item.as_bytes());
        let Some((key, value)) = pair.next() else {
            return Err(RequestError::Invalid);
        };
        if key == parameter {
            matches += 1;
            if matches > 1 {
                return Err(RequestError::Unauthorized);
            }
            if let Some(secret) = replacement {
                let encoded = Zeroizing::new(
                    form_urlencoded::byte_serialize(secret.as_bytes()).collect::<String>(),
                );
                let raw_key = item.split_once('=').map_or(item, |(key, _)| key);
                rewritten.push(format!("{raw_key}={}", encoded.as_str()));
            } else {
                found = Some(value.into_owned());
                rewritten.push(item.to_owned());
            }
        } else {
            rewritten.push(item.to_owned());
        }
    }
    if replacement.is_some() {
        if matches != 1 {
            return Err(RequestError::Unauthorized);
        }
        Ok(Some(Zeroizing::new(format!(
            "{path}?{}",
            rewritten.join("&")
        ))))
    } else {
        let token = found.filter(|token| !token.is_empty()).map(Zeroizing::new);
        Ok(token)
    }
}

fn encode_path_segment(value: &str) -> String {
    form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

fn decode_path(path: &str) -> Option<String> {
    let input = path.as_bytes();
    let mut output = Vec::with_capacity(input.len());
    let mut index = 0;
    while index < input.len() {
        if input[index] == b'%' {
            let high = *input.get(index + 1)?;
            let low = *input.get(index + 2)?;
            let decoded = hex_digit(high)? * 16 + hex_digit(low)?;
            if matches!(decoded, b'/' | b'\\') {
                return None;
            }
            output.push(decoded);
            index += 3;
        } else {
            output.push(input[index]);
            index += 1;
        }
    }
    let path = String::from_utf8(output).ok()?;
    let segments: Vec<_> = path.split('/').skip(1).collect();
    if !path.starts_with('/')
        || path
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace() || byte == b'\\')
        || path.split('/').any(|segment| matches!(segment, "." | ".."))
        || segments
            .iter()
            .enumerate()
            .any(|(index, segment)| segment.is_empty() && index + 1 != segments.len())
    {
        return None;
    }
    Some(path)
}

fn valid_http_token(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte))
}

fn hex_digit(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn tls_connector() -> io::Result<TlsConnector> {
    let native = rustls_native_certs::load_native_certs();
    let mut roots = RootCertStore::empty();
    for certificate in native.certs {
        roots.add(certificate).map_err(io::Error::other)?;
    }
    if roots.is_empty() {
        return Err(io::Error::other("No trusted TLS certificates were found"));
    }
    let config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(TlsConnector::from(Arc::new(config)))
}

async fn read_header(reader: &mut (impl AsyncRead + Unpin), limit: usize) -> io::Result<Vec<u8>> {
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
        if header.len() == limit {
            return Err(io::Error::other("HTTP header exceeds the size limit"));
        }
        let mut byte = [0];
        reader.read_exact(&mut byte).await?;
        header.push(byte[0]);
    }
    Ok(header)
}

fn close_response_connection(header: &[u8]) -> io::Result<Vec<u8>> {
    let text = std::str::from_utf8(header).map_err(io::Error::other)?;
    let mut lines = text.split("\r\n");
    let status = lines
        .next()
        .ok_or_else(|| io::Error::other("Invalid upstream response"))?;
    let mut output = format!("{status}\r\n");
    for line in lines.take_while(|line| !line.is_empty()) {
        let (name, _) = line
            .split_once(':')
            .ok_or_else(|| io::Error::other("Invalid upstream response header"))?;
        if !name.eq_ignore_ascii_case("connection")
            && !name.eq_ignore_ascii_case("keep-alive")
            && !name.eq_ignore_ascii_case("proxy-connection")
        {
            output.push_str(line);
            output.push_str("\r\n");
        }
    }
    output.push_str("Connection: close\r\n\r\n");
    Ok(output.into_bytes())
}

async fn response(client: &mut TcpStream, status: u16, message: &str) -> io::Result<()> {
    let body = message.as_bytes();
    client
        .write_all(
            format!(
                "HTTP/1.1 {status} {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                match status {
                    408 => "Request Timeout",
                    407 => "Proxy Authentication Required",
                    403 => "Forbidden",
                    502 => "Bad Gateway",
                    _ => "Bad Request",
                },
                body.len()
            )
            .as_bytes(),
        )
        .await?;
    client.write_all(body).await?;
    client.shutdown().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn broker_rejects_requests_without_a_session_token() {
        keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
        keyring::Entry::new("solmu-boxer", "OPENAI_API_KEY")
            .unwrap()
            .set_password("fixture-real-secret")
            .unwrap();
        let (broker, _) = Broker::start(
            &["openai".to_owned()],
            &BTreeMap::new(),
            BrokerOptions {
                endpoint_rules: &[],
                upstream_proxy: None,
                upstream_bypass: &[],
                denied_hosts: &[],
                reserved_ports: &[],
            },
        )
        .unwrap();
        let mut client = std::net::TcpStream::connect(("127.0.0.1", broker.port())).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        client
            .write_all(
                b"GET /openai/v1/models HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer invalid\r\nConnection: close\r\n\r\n",
            )
            .unwrap();
        let mut response = String::new();
        std::io::Read::read_to_string(&mut client, &mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 407"), "{response}");
        assert!(!response.contains("fixture-real-secret"));
    }

    fn credentials(provider: CredentialProvider) -> HashMap<String, Arc<Credential>> {
        HashMap::from([(
            provider.route().to_owned(),
            Arc::new(Credential {
                name: provider.name().to_owned(),
                authority: provider.host().to_owned(),
                host: provider.host().trim_end_matches(":443").to_owned(),
                incoming_header: builtin_route(provider).1.to_owned(),
                credential_format: builtin_route(provider).2.to_owned(),
                inject_mode: CredentialInjectionMode::Header,
                path_pattern: None,
                path_replacement: None,
                query_param_name: None,
                secret: Zeroizing::new("real-secret".to_owned()),
                token: Zeroizing::new("session-token".to_owned()),
            }),
        )])
    }

    fn custom_credentials() -> HashMap<String, Arc<Credential>> {
        HashMap::from([(
            "example_api".to_owned(),
            Arc::new(Credential {
                name: "example_api".to_owned(),
                authority: "api.example.com:443".to_owned(),
                host: "api.example.com".to_owned(),
                incoming_header: "X-API-Key".to_owned(),
                credential_format: "Key {}".to_owned(),
                inject_mode: CredentialInjectionMode::Header,
                path_pattern: None,
                path_replacement: None,
                query_param_name: None,
                secret: Zeroizing::new("real-secret".to_owned()),
                token: Zeroizing::new("session-token".to_owned()),
            }),
        )])
    }

    #[allow(clippy::too_many_arguments)]
    fn custom_mode_credential(
        name: &str,
        mode: CredentialInjectionMode,
        secret: &str,
        header: &str,
        format: &str,
        path_pattern: Option<&str>,
        path_replacement: Option<&str>,
        query_param_name: Option<&str>,
    ) -> HashMap<String, Arc<Credential>> {
        HashMap::from([(
            name.to_owned(),
            Arc::new(Credential {
                name: name.to_owned(),
                authority: "api.example.com:443".to_owned(),
                host: "api.example.com".to_owned(),
                incoming_header: header.to_owned(),
                credential_format: format.to_owned(),
                inject_mode: mode,
                path_pattern: path_pattern.map(str::to_owned),
                path_replacement: path_replacement.map(str::to_owned),
                query_param_name: query_param_name.map(str::to_owned),
                secret: Zeroizing::new(secret.to_owned()),
                token: Zeroizing::new("session-token".to_owned()),
            }),
        )])
    }

    #[test]
    fn openai_proxy_replaces_phantom_token_and_pins_upstream_host() {
        let request = parse_request(
            b"POST /openai/v1/chat/completions?stream=true HTTP/1.1\r\nAuthorization: Bearer session-token\r\nHost: 127.0.0.1\r\nContent-Length: 2\r\n\r\n",
            &credentials(CredentialProvider::Openai),
            &[],
        )
        .unwrap();
        assert!(request.upstream_header.contains("Host: api.openai.com\r\n"));
        assert!(
            request
                .upstream_header
                .contains("Authorization: Bearer real-secret\r\n")
        );
        assert!(
            request
                .upstream_header
                .contains("/v1/chat/completions?stream=true")
        );
        assert!(!request.upstream_header.contains("session-token"));
    }

    #[test]
    fn gemini_proxy_replaces_phantom_token_with_google_api_key_header() {
        let request = parse_request(
            b"POST /gemini/v1beta/models/gemini:generateContent HTTP/1.1\r\nx-goog-api-key: session-token\r\nHost: 127.0.0.1\r\nContent-Length: 0\r\n\r\n",
            &credentials(CredentialProvider::Gemini),
            &[],
        )
        .unwrap();
        assert!(
            request
                .upstream_header
                .contains("Host: generativelanguage.googleapis.com\r\n")
        );
        assert!(
            request
                .upstream_header
                .contains("x-goog-api-key: real-secret\r\n")
        );
        assert!(
            request
                .upstream_header
                .contains("/v1beta/models/gemini:generateContent")
        );
        assert!(!request.upstream_header.contains("session-token"));
    }

    #[test]
    fn github_proxy_uses_token_authorization_and_pins_upstream_host() {
        let request = parse_request(
            b"GET /github/user HTTP/1.1\r\nAuthorization: token session-token\r\nHost: 127.0.0.1\r\n\r\n",
            &credentials(CredentialProvider::Github),
            &[],
        )
        .unwrap();
        assert!(request.upstream_header.contains("Host: api.github.com\r\n"));
        assert!(
            request
                .upstream_header
                .contains("Authorization: token real-secret\r\n")
        );
        assert!(request.upstream_header.contains("GET /user HTTP/1.1"));
        assert!(!request.upstream_header.contains("session-token"));
    }

    #[test]
    fn gitlab_proxy_uses_bearer_authorization_and_keeps_api_path() {
        let request = parse_request(
            b"GET /gitlab/api/v4/projects HTTP/1.1\r\nAuthorization: Bearer session-token\r\nHost: 127.0.0.1\r\n\r\n",
            &credentials(CredentialProvider::Gitlab),
            &[],
        )
        .unwrap();
        assert!(request.upstream_header.contains("Host: gitlab.com\r\n"));
        assert!(
            request
                .upstream_header
                .contains("Authorization: Bearer real-secret\r\n")
        );
        assert!(
            request
                .upstream_header
                .contains("GET /api/v4/projects HTTP/1.1")
        );
        assert!(!request.upstream_header.contains("session-token"));
    }

    #[test]
    fn custom_proxy_route_pins_host_filters_endpoints_and_injects_configured_header() {
        let rules = vec![EndpointRule::parse("example_api:GET:/v1/**").unwrap()];
        let request = parse_request(
            b"GET /example_api/v1/models HTTP/1.1\r\nX-API-Key: Key session-token\r\nHost: 127.0.0.1\r\n\r\n",
            &custom_credentials(),
            &rules,
        )
        .unwrap();
        assert!(
            request
                .upstream_header
                .contains("Host: api.example.com\r\n")
        );
        assert!(
            request
                .upstream_header
                .contains("X-API-Key: Key real-secret\r\n")
        );
        assert!(request.upstream_header.contains("GET /v1/models HTTP/1.1"));
        assert!(!request.upstream_header.contains("session-token"));

        let denied = parse_request(
            b"GET /example_api/admin HTTP/1.1\r\nX-API-Key: Key session-token\r\nHost: 127.0.0.1\r\n\r\n",
            &custom_credentials(),
            &rules,
        );
        assert!(matches!(denied, Err(RequestError::Forbidden)));
    }

    #[test]
    fn custom_url_path_credential_is_checked_then_replaced() {
        let request = parse_request(
            b"POST /telegram/v1/botwrong/send HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
            &custom_mode_credential(
                "telegram",
                CredentialInjectionMode::UrlPath,
                "123456:secret",
                "Authorization",
                "Bearer {}",
                Some("/v1/bot{}/"),
                None,
                None,
            ),
            &[],
        );
        assert!(matches!(request, Err(RequestError::Unauthorized)));

        let request = parse_request(
            b"POST /telegram/v1/botsession-token/send HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
            &custom_mode_credential(
                "telegram",
                CredentialInjectionMode::UrlPath,
                "123456:secret",
                "Authorization",
                "Bearer {}",
                Some("/v1/bot{}/"),
                None,
                None,
            ),
            &[],
        )
        .unwrap();
        assert!(
            request
                .upstream_header
                .contains("/v1/bot123456%3Asecret/send")
        );
        assert!(!request.upstream_header.contains("session-token"));
    }

    #[test]
    fn custom_query_credential_is_checked_and_url_encoded() {
        let request = parse_request(
            b"GET /maps/v1/places?key=session-token&address=1%20Main HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
            &custom_mode_credential(
                "maps",
                CredentialInjectionMode::QueryParam,
                "a+b&c",
                "Authorization",
                "Bearer {}",
                None,
                None,
                Some("key"),
            ),
            &[],
        )
        .unwrap();
        assert!(
            request
                .upstream_header
                .contains("/v1/places?key=a%2Bb%26c&address=1%20Main")
        );
        assert!(!request.upstream_header.contains("session-token"));
    }

    #[test]
    fn custom_basic_auth_replaces_encoded_phantom_token() {
        let phantom = base64::engine::general_purpose::STANDARD.encode("session-token");
        let request = parse_request(
            format!("GET /private/resource HTTP/1.1\r\nAuthorization: Basic {phantom}\r\nHost: 127.0.0.1\r\n\r\n").as_bytes(),
            &custom_mode_credential(
                "private",
                CredentialInjectionMode::BasicAuth,
                "user:password",
                "Authorization",
                "",
                None,
                None,
                None,
            ),
            &[],
        )
        .unwrap();
        assert!(
            request
                .upstream_header
                .contains("Authorization: Basic dXNlcjpwYXNzd29yZA==\r\n")
        );
        assert!(!request.upstream_header.contains(&phantom));
    }

    #[test]
    fn invalid_or_missing_session_token_is_rejected() {
        let result = parse_request(
            b"POST /openai/v1/chat/completions HTTP/1.1\r\nAuthorization: Bearer wrong\r\n\r\n",
            &credentials(CredentialProvider::Openai),
            &[],
        );
        assert!(matches!(result, Err(RequestError::Unauthorized)));
    }

    #[test]
    fn bearer_scheme_is_case_insensitive_and_transfer_encoding_is_unique() {
        let request = parse_request(
            b"POST /openai/v1/chat/completions HTTP/1.1\r\nAuthorization: bEaReR session-token\r\nContent-Length: 0\r\n\r\n",
            &credentials(CredentialProvider::Openai),
            &[],
        );
        assert!(request.is_ok());

        let duplicate = parse_request(
            b"POST /openai/v1/chat/completions HTTP/1.1\r\nAuthorization: Bearer session-token\r\nTransfer-Encoding: chunked\r\nTransfer-Encoding: chunked\r\n\r\n",
            &credentials(CredentialProvider::Openai),
            &[],
        );
        assert!(matches!(duplicate, Err(RequestError::Invalid)));
    }

    #[test]
    fn bodyless_requests_do_not_wait_for_eof() {
        let credentials = credentials(CredentialProvider::Openai);
        let post = parse_request(
            b"POST /openai/v1/chat/completions HTTP/1.1\r\nAuthorization: Bearer session-token\r\n\r\n",
            &credentials,
            &[],
        )
        .unwrap();
        assert_eq!(post.body_limit, 0);

        let get = parse_request(
            b"GET /openai/v1/models HTTP/1.1\r\nAuthorization: Bearer session-token\r\n\r\n",
            &credentials,
            &[],
        )
        .unwrap();
        assert_eq!(get.body_limit, 0);
    }

    #[test]
    fn endpoint_allowlist_blocks_other_paths_and_matches_decoded_path_segments() {
        let credentials = credentials(CredentialProvider::Openai);
        let rules = vec![EndpointRule::parse("openai:POST:/v1/chat/completions").unwrap()];
        let allowed = parse_request(
            b"POST /openai/v1/%63hat/completions?stream=true HTTP/1.1\r\nAuthorization: Bearer session-token\r\nContent-Length: 0\r\n\r\n",
            &credentials,
            &rules,
        );
        assert!(allowed.is_ok());

        let denied = parse_request(
            b"GET /openai/v1/models HTTP/1.1\r\nAuthorization: Bearer session-token\r\n\r\n",
            &credentials,
            &rules,
        );
        assert!(matches!(denied, Err(RequestError::Forbidden)));
    }

    #[test]
    fn endpoint_path_decoding_rejects_encoded_separators_and_traversal() {
        assert!(decode_path("/v1/chat%2Fcompletions").is_none());
        assert!(decode_path("/v1/chat%5ccompletions").is_none());
        assert!(decode_path("/v1/%2e%2e/admin").is_none());
        assert_eq!(decode_path("/v1/chat%2Fcompletions"), None);
        assert_eq!(decode_path("/v1/chat%20completions"), None);
    }
}
