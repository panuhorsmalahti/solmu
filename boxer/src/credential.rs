use keyring::Entry;
use std::{
    ffi::OsString,
    fs::File,
    io::{self, Read},
    process::Command,
};
use zeroize::{Zeroize, Zeroizing};

const SERVICE: &str = "solmu-boxer";
const TARGET: &str = "default";
const MAX_FILE_SECRET_BYTES: u64 = 1024 * 1024;

pub fn command(args: &[OsString]) -> io::Result<i32> {
    match args.get(1).and_then(|arg| arg.to_str()) {
        Some("set") if args.len() == 3 => {
            let name = required_name(&args[2])?;
            let mut password = Zeroizing::new(
                rpassword::prompt_password(format!("Credential for {name}: "))
                    .map_err(io::Error::other)?,
            );
            if password.is_empty() {
                return Err(io::Error::other("Credential cannot be empty"));
            }
            entry(&name)?
                .set_password(&password)
                .map_err(|_| store_error(&name, "save"))?;
            password.zeroize();
            println!("Saved credential {name}");
            Ok(0)
        }
        Some("status") if args.len() == 3 => {
            let name = required_name(&args[2])?;
            match entry(&name)?.get_password() {
                Ok(mut password) => {
                    password.zeroize();
                    println!("Credential {name} is set");
                    Ok(0)
                }
                Err(keyring::Error::NoEntry) => {
                    println!("Credential {name} is not set");
                    Ok(1)
                }
                Err(_) => Err(store_error(&name, "read")),
            }
        }
        Some("delete") if args.len() == 3 => {
            let name = required_name(&args[2])?;
            entry(&name)?
                .delete_credential()
                .map_err(|_| store_error(&name, "delete"))?;
            println!("Deleted credential {name}");
            Ok(0)
        }
        _ => Err(usage()),
    }
}

pub fn load(names: &[String]) -> io::Result<Vec<(String, Zeroizing<String>)>> {
    load_with(names, |name| {
        if let Some(variable) = environment_reference(name) {
            return std::env::var(variable).map_err(|_| {
                io::Error::other(format!(
                    "Credential environment variable {variable} is not set"
                ))
            });
        }
        if is_file_reference(name) {
            return read_file_secret(name);
        }
        if let Some((service, account)) = keyring_reference(name) {
            return read_keyring(&service, &account);
        }
        if let Some((item, field)) = bitwarden_reference(name) {
            return read_bitwarden(&item, field.as_deref());
        }
        if name.starts_with("op://") {
            return read_onepassword(name);
        }
        if name.starts_with("apple-password://") {
            return read_apple_password(name);
        }
        match entry(name)?.get_password() {
            Ok(password) => Ok(password),
            Err(keyring::Error::NoEntry) => Err(io::Error::other(format!(
                "Credential {name} is not set; run `boxer credential set {name}`"
            ))),
            Err(_) => Err(store_error(name, "read")),
        }
    })
}

pub fn is_onepassword_reference(value: &str) -> bool {
    if value.chars().any(char::is_control) {
        return false;
    }
    let Ok(url) = url::Url::parse(value) else {
        return false;
    };
    url.scheme() == "op"
        && url.host_str().is_some_and(|vault| !vault.is_empty())
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.fragment().is_none()
        && url.path_segments().is_some_and(|mut segments| {
            segments.next().is_some_and(|item| !item.is_empty())
                && segments.next().is_some_and(|field| !field.is_empty())
                && segments.next().is_none()
        })
}

pub fn valid_source_key(value: &str) -> bool {
    valid_name(value)
        || environment_reference(value).is_some()
        || is_file_reference(value)
        || keyring_reference(value).is_some()
        || bitwarden_reference(value).is_some()
        || is_onepassword_reference(value)
        || is_apple_password_reference(value)
}

pub fn keyring_reference(value: &str) -> Option<(String, String)> {
    let reference = value.strip_prefix("keyring://")?;
    if reference.chars().any(char::is_control) || reference.contains(['?', '#']) {
        return None;
    }
    let (service, account) = reference.split_once('/')?;
    if service.is_empty()
        || account.contains('/')
        || !service
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return None;
    }
    let account = percent_encoding::percent_decode_str(account)
        .decode_utf8()
        .ok()?
        .into_owned();
    if account.is_empty() || account.chars().any(char::is_control) {
        return None;
    }
    Some((service.to_owned(), account))
}

fn read_keyring(service: &str, account: &str) -> io::Result<String> {
    match keyring_entry(service, account)?.get_password() {
        Ok(password) => Ok(password),
        Err(keyring::Error::NoEntry) => Err(io::Error::other(format!(
            "Credential {account} is not set in keyring service {service}"
        ))),
        Err(_) => Err(io::Error::other(format!(
            "Could not read credential {account} from keyring service {service}"
        ))),
    }
}

pub fn bitwarden_reference(value: &str) -> Option<(String, Option<String>)> {
    let reference = value.strip_prefix("bw://")?;
    if reference.chars().any(char::is_control) || reference.contains(['?', '#']) {
        return None;
    }
    let (item, field) = match reference.split_once('/') {
        Some((item, field)) if !item.is_empty() && !field.is_empty() && !field.contains('/') => {
            (item, Some(field))
        }
        Some(_) => return None,
        None if !reference.is_empty() => (reference, None),
        None => return None,
    };
    if !item
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return None;
    }
    let field = match field {
        Some(field) => Some(
            percent_encoding::percent_decode_str(field)
                .decode_utf8()
                .ok()?
                .into_owned(),
        ),
        None => None,
    };
    if field
        .as_deref()
        .is_some_and(|field| field.is_empty() || field.chars().any(char::is_control))
    {
        return None;
    }
    Some((item.to_owned(), field))
}

fn read_bitwarden(item_id: &str, field: Option<&str>) -> io::Result<String> {
    read_bitwarden_with(item_id, field, |item| {
        let mut output = Command::new("bw")
            .args(["get", "item", item])
            .stdin(std::process::Stdio::null())
            .output()
            .map_err(|_| io::Error::other("Could not run the Bitwarden CLI"))?;
        if !output.status.success() {
            output.stdout.zeroize();
            output.stderr.zeroize();
            return Err(io::Error::other(
                "Could not read the Bitwarden item; check that `bw` is installed and unlocked",
            ));
        }
        let stdout = std::mem::take(&mut output.stdout);
        output.stderr.zeroize();
        Ok(stdout)
    })
}

fn read_bitwarden_with(
    item_id: &str,
    field: Option<&str>,
    read: impl FnOnce(&str) -> io::Result<Vec<u8>>,
) -> io::Result<String> {
    let mut bytes = read(item_id)?;
    parse_bitwarden_item(&mut bytes, field)
}

fn parse_bitwarden_item(bytes: &mut Vec<u8>, field: Option<&str>) -> io::Result<String> {
    let mut item: serde_json::Value = match serde_json::from_slice(bytes) {
        Ok(item) => item,
        Err(_) => {
            bytes.zeroize();
            return Err(io::Error::other(
                "Bitwarden returned an invalid item; check that `bw` is unlocked",
            ));
        }
    };
    bytes.zeroize();
    let secret = bitwarden_field(&item, field).map(str::to_owned);
    zeroize_json_strings(&mut item);
    secret.ok_or_else(|| io::Error::other("The requested Bitwarden item field is empty or missing"))
}

fn bitwarden_field<'a>(item: &'a serde_json::Value, field: Option<&str>) -> Option<&'a str> {
    let login = item.get("login");
    match field {
        Some("password") => login?.get("password")?.as_str(),
        Some("username") => login?.get("username")?.as_str(),
        Some("name") => item.get("name")?.as_str(),
        Some("notes") => item.get("notes")?.as_str(),
        Some(field) => item
            .get("fields")?
            .as_array()?
            .iter()
            .find(|entry| entry.get("name").and_then(serde_json::Value::as_str) == Some(field))?
            .get("value")?
            .as_str(),
        None => login
            .and_then(|login| login.get("password"))
            .and_then(serde_json::Value::as_str)
            .or_else(|| {
                item.get("fields")?
                    .as_array()?
                    .iter()
                    .find_map(|entry| entry.get("value").and_then(serde_json::Value::as_str))
            }),
    }
}

fn zeroize_json_strings(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::String(value) => value.zeroize(),
        serde_json::Value::Array(values) => values.iter_mut().for_each(zeroize_json_strings),
        serde_json::Value::Object(values) => values.values_mut().for_each(zeroize_json_strings),
        _ => {}
    }
}

pub fn environment_reference(value: &str) -> Option<&str> {
    let variable = value.strip_prefix("env://")?;
    valid_name(variable).then_some(variable)
}

pub fn is_file_reference(value: &str) -> bool {
    if value.chars().any(char::is_control) {
        return false;
    }
    let Ok(url) = url::Url::parse(value) else {
        return false;
    };
    url.scheme() == "file"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && url
            .host_str()
            .is_none_or(|host| host.eq_ignore_ascii_case("localhost"))
        && url.to_file_path().is_ok()
}

fn read_file_secret(reference: &str) -> io::Result<String> {
    if !is_file_reference(reference) {
        return Err(io::Error::other("Invalid file credential reference"));
    }
    let url = url::Url::parse(reference)
        .map_err(|_| io::Error::other("Invalid file credential reference"))?;
    let path = url
        .to_file_path()
        .map_err(|_| io::Error::other("Invalid file credential reference"))?;
    let file = File::open(path)
        .map_err(|_| io::Error::other("Could not read the file credential source"))?;
    let mut bytes = Vec::new();
    if file
        .take(MAX_FILE_SECRET_BYTES + 1)
        .read_to_end(&mut bytes)
        .is_err()
    {
        bytes.zeroize();
        return Err(io::Error::other(
            "Could not read the file credential source",
        ));
    }
    if bytes.len() as u64 > MAX_FILE_SECRET_BYTES {
        bytes.zeroize();
        return Err(io::Error::other("File credential source exceeds 1 MiB"));
    }
    if bytes.ends_with(b"\n") {
        bytes.pop();
        if bytes.ends_with(b"\r") {
            bytes.pop();
        }
    }
    match String::from_utf8(std::mem::take(&mut bytes)) {
        Ok(secret) => Ok(secret),
        Err(error) => {
            let mut bytes = error.into_bytes();
            bytes.zeroize();
            Err(io::Error::other(
                "File credential source is not valid UTF-8",
            ))
        }
    }
}

pub fn is_apple_password_reference(value: &str) -> bool {
    if value.chars().any(char::is_control) {
        return false;
    }
    let Ok(url) = url::Url::parse(value) else {
        return false;
    };
    url.scheme() == "apple-password"
        && url.host_str().is_some_and(|server| !server.is_empty())
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && url.path_segments().is_some_and(|mut segments| {
            segments.next().is_some_and(|account| {
                !account.is_empty()
                    && !percent_encoding::percent_decode_str(account)
                        .decode_utf8_lossy()
                        .chars()
                        .any(char::is_control)
            }) && segments.next().is_none()
        })
}

fn read_onepassword(reference: &str) -> io::Result<String> {
    if !is_onepassword_reference(reference) {
        return Err(io::Error::other("Invalid 1Password secret reference"));
    }
    let mut output = Command::new("op")
        .args(["read", "--no-newline", reference])
        .output()
        .map_err(|_| io::Error::other("Could not read secret using the 1Password CLI"))?;
    if !output.status.success() {
        output.stdout.zeroize();
        return Err(io::Error::other(
            "Could not read secret using the 1Password CLI; check that `op` is installed and signed in",
        ));
    }
    match String::from_utf8(std::mem::take(&mut output.stdout)) {
        Ok(secret) => Ok(secret),
        Err(error) => {
            let mut bytes = error.into_bytes();
            bytes.zeroize();
            Err(io::Error::other(
                "1Password returned a secret that is not valid UTF-8",
            ))
        }
    }
}

fn read_apple_password(reference: &str) -> io::Result<String> {
    if !is_apple_password_reference(reference) {
        return Err(io::Error::other("Invalid Apple Passwords reference"));
    }
    #[cfg(target_os = "macos")]
    {
        let url = url::Url::parse(reference).map_err(io::Error::other)?;
        let server = url
            .host_str()
            .ok_or_else(|| io::Error::other("Apple Passwords server is missing"))?;
        let account = url
            .path_segments()
            .and_then(|mut segments| segments.next())
            .ok_or_else(|| io::Error::other("Apple Passwords account is missing"))?;
        let account = percent_encoding::percent_decode_str(account)
            .decode_utf8()
            .map_err(io::Error::other)?;
        let mut output = Command::new("security")
            .arg("find-internet-password")
            .arg("-s")
            .arg(server)
            .arg("-a")
            .arg(account.as_ref())
            .arg("-w")
            .output()
            .map_err(|_| io::Error::other("Could not read from Apple Passwords"))?;
        if !output.status.success() {
            output.stdout.zeroize();
            return Err(io::Error::other(
                "Could not read from Apple Passwords; check that the item is available in Keychain",
            ));
        }
        if output.stdout.ends_with(b"\n") {
            output.stdout.pop();
            if output.stdout.ends_with(b"\r") {
                output.stdout.pop();
            }
        }
        match String::from_utf8(std::mem::take(&mut output.stdout)) {
            Ok(secret) => Ok(secret),
            Err(error) => {
                let mut bytes = error.into_bytes();
                bytes.zeroize();
                Err(io::Error::other(
                    "Apple Passwords returned a secret that is not valid UTF-8",
                ))
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err(io::Error::other(
            "apple-password:// references are only supported on macOS",
        ))
    }
}

fn load_with(
    names: &[String],
    mut read: impl FnMut(&str) -> io::Result<String>,
) -> io::Result<Vec<(String, Zeroizing<String>)>> {
    names
        .iter()
        .map(|name| {
            let password = read(name)?;
            if password.is_empty() {
                return Err(io::Error::other(format!(
                    "Credential {name} is empty; update it with `boxer credential set {name}`"
                )));
            }
            Ok((name.clone(), Zeroizing::new(password)))
        })
        .collect()
}

fn entry(name: &str) -> io::Result<Entry> {
    keyring_entry(SERVICE, name).map_err(|_| store_error(name, "open"))
}

fn keyring_entry(service: &str, account: &str) -> io::Result<Entry> {
    Entry::new_with_target(TARGET, service, account).map_err(|_| store_error(account, "open"))
}

fn required_name(argument: &OsString) -> io::Result<String> {
    let name = argument
        .to_str()
        .ok_or_else(|| io::Error::other("Credential name must be valid Unicode"))?;
    if !valid_name(name) {
        return Err(io::Error::other(
            "Credential name must be a valid environment variable name",
        ));
    }
    Ok(name.to_owned())
}

pub fn valid_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    if !bytes
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == b'_')
        || !bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return false;
    }
    let upper = name.to_ascii_uppercase();
    !matches!(
        upper.as_str(),
        "PATH"
            | "HOME"
            | "USERPROFILE"
            | "APPDATA"
            | "LOCALAPPDATA"
            | "SYSTEMROOT"
            | "WINDIR"
            | "COMSPEC"
            | "PATHEXT"
            | "TMP"
            | "TEMP"
            | "TMPDIR"
            | "PWD"
            | "OLDPWD"
            | "SHELL"
            | "SSH_AUTH_SOCK"
            | "LD_PRELOAD"
            | "LD_LIBRARY_PATH"
            | "DYLD_INSERT_LIBRARIES"
            | "HTTP_PROXY"
            | "HTTPS_PROXY"
            | "ALL_PROXY"
            | "NO_PROXY"
    ) && !upper.starts_with("SOLMU_")
        && !upper.starts_with("LLM_")
        && !upper.starts_with("BOXER_")
}

fn store_error(name: &str, action: &str) -> io::Error {
    io::Error::other(format!(
        "Could not {action} credential {name} in the system credential store"
    ))
}

fn usage() -> io::Error {
    io::Error::other(
        "Usage: boxer credential set|status|delete ENV_NAME (the credential value is never printed)",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_are_loaded_from_the_mockable_platform_store() {
        keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
        let name = "BOXER_TEST_API_KEY";
        let mut loaded = load_with(&[name.to_owned()], |name| {
            let entry = entry(name)?;
            entry.set_password("test-secret").unwrap();
            entry.get_password().map_err(io::Error::other)
        })
        .unwrap();
        assert_eq!(loaded[0].0, name);
        assert_eq!(loaded[0].1.as_str(), "test-secret");
        loaded[0].1.zeroize();
    }

    #[test]
    fn credential_names_cannot_override_runtime_configuration() {
        for name in [
            "",
            "9TOKEN",
            "OPENAI-API-KEY",
            "PATH",
            "path",
            "HOME",
            "SOLMU_WORKSPACE",
            "LLM_MODEL",
            "BOXER_CGROUP_ROOT",
            "LD_PRELOAD",
        ] {
            assert!(!valid_name(name), "{name} must be rejected");
        }
        assert!(valid_name("OPENAI_API_KEY"));
        assert!(valid_name("DATABASE_PASSWORD"));
    }

    #[test]
    fn onepassword_secret_references_require_a_vault_item_and_field() {
        assert!(is_onepassword_reference(
            "op://Development/OpenAI API Key/credential"
        ));
        for invalid in [
            "op://",
            "op://vault/item",
            "op://vault//field",
            "op://user@vault/item/field",
            "op://vault/item/field#fragment",
        ] {
            assert!(!is_onepassword_reference(invalid), "accepted {invalid}");
        }
        assert!(is_onepassword_reference(
            "op://vault/item/one-time%20password?attribute=otp"
        ));
    }

    #[test]
    fn apple_password_references_require_server_and_account() {
        assert!(is_apple_password_reference(
            "apple-password://github.com/alice%40example.com"
        ));
        for invalid in [
            "apple-password://",
            "apple-password://github.com",
            "apple-password://github.com/",
            "apple-password://user@github.com/account",
            "apple-password://github.com/account?query=value",
        ] {
            assert!(!is_apple_password_reference(invalid), "accepted {invalid}");
        }
    }

    #[test]
    fn environment_secret_references_preserve_variable_case_and_validate_names() {
        assert_eq!(
            environment_reference("env://Mixed_Case1"),
            Some("Mixed_Case1")
        );
        for invalid in [
            "env://",
            "env://PATH",
            "env://SOLMU_WORKSPACE",
            "env://BAD-NAME",
        ] {
            assert!(
                environment_reference(invalid).is_none(),
                "accepted {invalid}"
            );
        }
    }

    #[test]
    fn file_secret_references_are_local_and_read_one_trailing_newline() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("secret.txt");
        std::fs::write(&path, b"file-secret\r\n").unwrap();
        let reference = url::Url::from_file_path(path).unwrap().to_string();
        assert!(is_file_reference(&reference));
        assert_eq!(read_file_secret(&reference).unwrap(), "file-secret");
        assert!(!is_file_reference("file://remote.example/secrets/key"));
        assert!(!is_file_reference("file:///etc/passwd?copy=1"));
    }

    #[test]
    fn custom_keyring_references_load_from_the_selected_service() {
        keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
        let (service, account) = keyring_reference("keyring://my-service/openai_api_key").unwrap();
        keyring_entry(&service, &account)
            .unwrap()
            .set_password("keyring-secret")
            .unwrap();
        let loaded = load(&["keyring://my-service/openai_api_key".to_owned()]).unwrap();
        assert_eq!(loaded[0].0, "keyring://my-service/openai_api_key");
        assert_eq!(loaded[0].1.as_str(), "keyring-secret");
        assert_eq!(
            keyring_reference("keyring://My-Service/user%40example.com").unwrap(),
            ("My-Service".to_owned(), "user@example.com".to_owned())
        );
        for invalid in [
            "keyring://",
            "keyring:///account",
            "keyring://service/",
            "keyring://service/account/extra",
            "keyring://service/account%2Fextra",
            "keyring://service/account?query=value",
        ] {
            assert!(keyring_reference(invalid).is_none(), "accepted {invalid}");
        }
    }

    #[test]
    fn bitwarden_references_load_passwords_and_named_fields() {
        let item = br#"{"login":{"username":"alice","password":"bw-password"},"name":"Example","notes":"memo","fields":[{"name":"api-key","value":"bw-custom-secret"}]}"#;
        assert_eq!(
            read_bitwarden_with("01234567-89ab-cdef-0123-456789abcdef", None, |_| {
                Ok(item.to_vec())
            })
            .unwrap(),
            "bw-password"
        );
        assert_eq!(
            read_bitwarden_with(
                "01234567-89ab-cdef-0123-456789abcdef",
                Some("api-key"),
                |_| { Ok(item.to_vec()) }
            )
            .unwrap(),
            "bw-custom-secret"
        );
        assert_eq!(
            bitwarden_reference("bw://01234567-89ab-cdef-0123-456789abcdef/api%20key"),
            Some((
                "01234567-89ab-cdef-0123-456789abcdef".to_owned(),
                Some("api key".to_owned())
            ))
        );
        for invalid in ["bw://", "bw://item/", "bw://item/a/b", "bw://item?field=x"] {
            assert!(bitwarden_reference(invalid).is_none(), "accepted {invalid}");
        }
    }
}
