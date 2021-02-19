use crate::models::*;
use git2::{build::RepoBuilder, Direction, FetchOptions, Repository};
use std::{self, collections::HashMap, error::Error, path::PathBuf};
use tempfile::TempDir;

pub fn clone_commit(
    url: &str,
    branch: &str,
    commit_hash: Option<String>,
    dir: &PathBuf,
    auth: Option<&GitAuth>,
) -> Result<(), Box<dyn Error>> {
    let mut callbacks = git2::RemoteCallbacks::new();
    callbacks.credentials(
        |_user: &str, user_from_url: Option<&str>, _cred: git2::CredentialType| match auth {
            Some(a) => match a {
                GitAuth::PrivateKey { path, passphrase } => git2::Cred::ssh_key(
                    user_from_url.unwrap_or("git"),
                    None,
                    std::path::Path::new(path),
                    passphrase.as_ref().map(|s| s.as_str()),
                ),
            },
            None => git2::Cred::default(),
        },
    );
    let mut fo = FetchOptions::new();
    fo.remote_callbacks(callbacks);

    let repo = RepoBuilder::new()
        .fetch_options(fo)
        .branch(branch)
        .clone(url, dir)?;

    if let Some(commit_hash) = commit_hash {
        let commit_hash = commit_hash.as_str();
        let oid = git2::Oid::from_str(commit_hash)?;
        let commit = repo.find_commit(oid)?;

        repo.branch(commit_hash, &commit, false)?;
        let obj = repo.revparse_single(&("refs/heads/".to_owned() + commit_hash))?;
        repo.checkout_tree(&obj, None)?;
        repo.set_head(&("refs/heads/".to_owned() + commit_hash))?;
    }

    Ok(())
}

pub fn get_branch_hashes(
    url: &str,
    auth: Option<&GitAuth>,
) -> Result<HashMap<String, String>, Box<dyn Error>> {
    let tmp_dir = TempDir::new()?;
    let repo = Repository::init(tmp_dir.path())?;

    let mut callbacks = git2::RemoteCallbacks::new();
    callbacks.credentials(
        |_user: &str, user_from_url: Option<&str>, _cred: git2::CredentialType| match auth {
            Some(a) => match a {
                GitAuth::PrivateKey { path, passphrase } => git2::Cred::ssh_key(
                    user_from_url.unwrap_or("git"),
                    None,
                    std::path::Path::new(path),
                    passphrase.as_ref().map(|s| s.as_str()),
                ),
            },
            None => git2::Cred::default(),
        },
    );

    let mut remote = repo.remote("origin", url)?;
    let connection = remote.connect_auth(Direction::Fetch, Some(callbacks), None)?;
    let mut hashes: HashMap<String, String> = HashMap::new();
    for b in connection.list()?.iter() {
        if b.name().starts_with("refs/heads/") {
            let bname = b.name()[11..].to_string();
            hashes.insert(bname, b.oid().to_string());
        }
    }

    Ok(hashes)
}
