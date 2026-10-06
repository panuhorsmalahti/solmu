use ring::{
    rand::SystemRandom,
    signature::{self, Ed25519KeyPair, KeyPair},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

const MAX_SIGNED_FILE: u64 = 32 * 1024 * 1024;
const DOMAIN: &[u8] = b"solmu-boxer-trust-v1\0";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TrustPolicy {
    version: u32,
    files: Vec<PathBuf>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SignatureFile {
    version: u32,
    digest_sha256: String,
    signature: String,
}

pub fn command(args: &[OsString]) -> io::Result<i32> {
    match args.get(1).and_then(|arg| arg.to_str()) {
        Some("help" | "--help" | "-h") if args.len() == 2 => {
            println!(
                "Boxer trust\n\nUsage:\n  boxer trust keygen --private-key FILE --public-key FILE\n  boxer trust sign --key PRIVATE_KEY FILE\n  boxer trust verify --key PUBLIC_KEY FILE\n\nSignatures use Ed25519 and are stored beside each file as FILE.boxer.sig."
            );
            Ok(0)
        }
        Some("keygen")
            if args.len() == 6 && args[2] == "--private-key" && args[4] == "--public-key" =>
        {
            keygen(Path::new(&args[3]), Path::new(&args[5]))
        }
        Some("sign") if args.len() == 5 && args[2] == "--key" => {
            sign(Path::new(&args[3]), Path::new(&args[4]))
        }
        Some("verify") if args.len() == 5 && args[2] == "--key" => {
            verify(Path::new(&args[3]), Path::new(&args[4])).map(|_| 0)
        }
        _ => Err(io::Error::other(
            "Usage: boxer trust keygen --private-key FILE --public-key FILE | sign --key PRIVATE_KEY FILE | verify --key PUBLIC_KEY FILE",
        )),
    }
}

pub fn verify_files(public_key_path: &Path, files: &[PathBuf]) -> io::Result<()> {
    for file in files {
        verify(public_key_path, file)?;
    }
    Ok(())
}

pub fn verify_policy(
    public_key_path: &Path,
    requested_policy: &Path,
    workspace: &Path,
) -> io::Result<()> {
    let policy_path = workspace_path(workspace, requested_policy)?;
    verify(public_key_path, &policy_path)?;
    let metadata = fs::metadata(&policy_path)?;
    if metadata.len() > 64 * 1024 {
        return Err(io::Error::other("Trust policy exceeds the 64 KiB limit"));
    }
    let policy: TrustPolicy =
        serde_json::from_slice(&fs::read(&policy_path)?).map_err(io::Error::other)?;
    if policy.version != 1 {
        return Err(io::Error::other("Unsupported trust policy version"));
    }
    if policy.files.is_empty() || policy.files.len() > 256 {
        return Err(io::Error::other(
            "Trust policy must list between 1 and 256 files",
        ));
    }
    let mut unique = std::collections::HashSet::new();
    let mut files = Vec::with_capacity(policy.files.len());
    for file in policy.files {
        let canonical = workspace_path(workspace, &file)?;
        if !unique.insert(canonical.clone()) {
            return Err(io::Error::other("Trust policy contains a duplicate file"));
        }
        files.push(canonical);
    }
    verify_files(public_key_path, &files)
}

fn workspace_path(workspace: &Path, requested: &Path) -> io::Result<PathBuf> {
    let requested = if requested.is_absolute() {
        requested.to_owned()
    } else {
        workspace.join(requested)
    };
    let canonical = requested.canonicalize().map_err(|error| {
        io::Error::other(format!(
            "Cannot resolve trusted workspace file {}: {error}",
            requested.display()
        ))
    })?;
    if !canonical.starts_with(workspace) || !canonical.is_file() {
        return Err(io::Error::other(
            "Trust policies and trusted files must be regular files inside the workspace",
        ));
    }
    Ok(canonical)
}

fn keygen(private_path: &Path, public_path: &Path) -> io::Result<i32> {
    if private_path == public_path {
        return Err(io::Error::other(
            "Private and public key paths must be different",
        ));
    }
    let rng = SystemRandom::new();
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng)
        .map_err(|_| io::Error::other("Could not generate an Ed25519 key"))?;
    let pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref())
        .map_err(|_| io::Error::other("Invalid generated Ed25519 key"))?;
    write_private(private_path, pkcs8.as_ref())?;
    if let Err(error) = write_new(
        public_path,
        hex::encode(pair.public_key().as_ref()).as_bytes(),
    ) {
        let _ = fs::remove_file(private_path);
        return Err(error);
    }
    println!(
        "Generated Ed25519 trust keys. Keep the private key secret; distribute the public key through a trusted channel."
    );
    Ok(0)
}

fn write_new(path: &Path, contents: &[u8]) -> io::Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(contents)?;
    file.sync_all()
}

fn write_private(path: &Path, contents: &[u8]) -> io::Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(contents)?;
        file.sync_all()
    }
    #[cfg(not(unix))]
    {
        write_new(path, contents)
    }
}

fn sign(private_path: &Path, file_path: &Path) -> io::Result<i32> {
    let private = fs::read(private_path)?;
    let pair = Ed25519KeyPair::from_pkcs8(&private)
        .map_err(|_| io::Error::other("Invalid Ed25519 private key"))?;
    let contents = read_limited(file_path)?;
    let digest = Sha256::digest(&contents);
    let message = signed_message(&digest);
    let signature = pair.sign(&message);
    let record = SignatureFile {
        version: 1,
        digest_sha256: hex::encode(digest),
        signature: hex::encode(signature.as_ref()),
    };
    let sidecar = sidecar(file_path);
    write_new(
        &sidecar,
        &serde_json::to_vec_pretty(&record).map_err(io::Error::other)?,
    )?;
    println!("Signed {}", file_path.display());
    Ok(0)
}

fn verify(public_path: &Path, file_path: &Path) -> io::Result<()> {
    let public = hex::decode(fs::read_to_string(public_path)?.trim()).map_err(io::Error::other)?;
    let signature_file = sidecar(file_path);
    let metadata = fs::metadata(&signature_file)?;
    if metadata.len() > 16 * 1024 {
        return Err(io::Error::other("Signature sidecar is too large"));
    }
    let record: SignatureFile =
        serde_json::from_slice(&fs::read(signature_file)?).map_err(io::Error::other)?;
    if record.version != 1 {
        return Err(io::Error::other("Unsupported signature version"));
    }
    let contents = read_limited(file_path)?;
    let digest = Sha256::digest(&contents);
    if hex::encode(digest) != record.digest_sha256 {
        return Err(io::Error::other(format!(
            "Trust verification failed: {} was changed after signing",
            file_path.display()
        )));
    }
    let signature = hex::decode(record.signature).map_err(io::Error::other)?;
    let message = signed_message(&digest);
    signature::UnparsedPublicKey::new(&signature::ED25519, public)
        .verify(&message, &signature)
        .map_err(|_| {
            io::Error::other(format!(
                "Trust verification failed: {} has no valid signature from the trusted key",
                file_path.display()
            ))
        })?;
    println!("Verified {}", file_path.display());
    Ok(())
}

fn read_limited(path: &Path) -> io::Result<Vec<u8>> {
    let file = fs::File::open(path)?;
    if file.metadata()?.len() > MAX_SIGNED_FILE {
        return Err(io::Error::other("Signed file exceeds the 32 MiB limit"));
    }
    let mut data = Vec::new();
    file.take(MAX_SIGNED_FILE + 1).read_to_end(&mut data)?;
    if data.len() as u64 > MAX_SIGNED_FILE {
        return Err(io::Error::other("Signed file exceeds the 32 MiB limit"));
    }
    Ok(data)
}

fn sidecar(path: &Path) -> PathBuf {
    let mut sidecar = path.as_os_str().to_owned();
    sidecar.push(".boxer.sig");
    PathBuf::from(sidecar)
}

fn signed_message(digest: &[u8]) -> Vec<u8> {
    let mut message = Vec::with_capacity(DOMAIN.len() + digest.len());
    message.extend_from_slice(DOMAIN);
    message.extend_from_slice(digest);
    message
}
