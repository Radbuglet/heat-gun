use std::{fs, io, path::PathBuf};

use anyhow::Context;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

pub type DevKeyPair = (PrivateKeyDer<'static>, CertificateDer<'static>);

pub fn generate_dev_priv_key() -> anyhow::Result<DevKeyPair> {
    let path = dev_pub_cert_path()?;

    // Generate self-signed certificate
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();

    // Extract private key
    let key = PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der());

    // Save public key
    let cert = CertificateDer::from(cert.cert);
    fs::write(&path, &cert)
        .with_context(|| format!("failed to write certificate at `{}`", path.display()))?;

    tracing::info!("Wrote development certificate to `{}`", path.display());

    Ok((key.into(), cert))
}

pub fn fetch_dev_pub_cert() -> anyhow::Result<Option<CertificateDer<'static>>> {
    let path = dev_pub_cert_path()?;

    match fs::read(&path) {
        Ok(cert) => Ok(Some(CertificateDer::from(cert))),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        err => err
            .map(|_| unreachable!())
            .with_context(|| format!("failed to read certificate at `{}`", path.display())),
    }
}

fn dev_pub_cert_path() -> anyhow::Result<PathBuf> {
    let path = directories_next::ProjectDirs::from("io.github", "radbuglet", "heat-gun")
        .context("failed to get project directory")?;

    let path = path.data_local_dir().join("dev_certificates");

    fs::create_dir_all(&path).with_context(|| {
        format!(
            "failed to create developer certificate directory at `{}`",
            path.display()
        )
    })?;

    Ok(path.join("cert.der"))
}
