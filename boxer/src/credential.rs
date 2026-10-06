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
        || is_onepassword_reference(value)
        || is_apple_password_reference(value)
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
    Entry::new_with_target(TARGET, SERVICE, name).map_err(|_| store_error(name, "open"))
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
}
