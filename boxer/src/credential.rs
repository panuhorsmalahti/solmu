use keyring::Entry;
use std::{ffi::OsString, io, process::Command};
use zeroize::{Zeroize, Zeroizing};

const SERVICE: &str = "solmu-boxer";
const TARGET: &str = "default";

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
        if name.starts_with("op://") {
            return read_onepassword(name);
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
}
