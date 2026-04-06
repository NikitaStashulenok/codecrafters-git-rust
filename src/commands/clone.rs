use std::{
    env,
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::Path,
};

use anyhow::Result;
use reqwest::{Client, StatusCode};

use crate::my_git::{
    object::{GitObject, ObjectContent},
    packfile::PackFile,
};

pub async fn invoke(url: String, dir: Option<String>) -> Result<()> {
    create_destination(&url, dir.as_ref())?;

    let (head, advertise) = ref_discovery(&url).await?;

    let mut pack = fetch_ref(&url, &advertise).await?;
    pack.parse()?;

    build_repository(&head)
}

fn create_destination(url: &str, dir: Option<&String>) -> Result<()> {
    let dirname = if let Some(dirname) = dir {
        dirname
    } else {
        let Some((_, dirname)) = url.trim_end_matches(".git").rsplit_once(r"/") else {
            anyhow::bail!("invalid url")
        };
        dirname
    };

    println!("Clone into: {}", dirname);

    let dst = Path::new(dirname);
    if dst.exists() {
        fs::remove_dir_all(dst)?;
    }

    fs::create_dir_all(dst)?;
    std::env::set_current_dir(dst)?;

    crate::my_git::commands::init::invoke()
}

async fn ref_discovery(url: &str) -> Result<(String, Vec<String>)> {
    let url = format!("{url}/info/refs?service=git-upload-pack");

    let client = Client::new();
    let res = client.get(&url).send().await?;

    let status = res.status();
    let res = res.text().await?;
    let refs: Vec<&str> = res.split('\n').collect();

    match status {
        StatusCode::OK | StatusCode::NOT_MODIFIED => {}
        _ => anyhow::bail!(format!("failed status code validation: {}", status)),
    }
    let rgx = regex::Regex::new(r"^[0-9a-f]{4}# service=git-upload-pack")?;
    if !rgx.is_match(refs[0]) {
        anyhow::bail!("failed regex validation");
    }

    let mut refs = refs[1..].iter();
    let Some(&head) = refs.next() else {
        anyhow::bail!("unknow refs header");
    };
    let Some((head, rest)) = head[8..].split_once(' ') else {
        anyhow::bail!("unknow refs header");
    };
    anyhow::ensure!(rest.contains("HEAD"));

    let mut discovered = Vec::new();
    for &reference in refs {
        if reference == "0000" {
            break;
        }
        let Some((reference, _)) = reference[4..].split_once(' ') else {
            anyhow::bail!("failed to parse refs");
        };
        discovered.push(reference.to_string());
    }

    Ok((head.to_string(), discovered))
}

async fn fetch_ref(url: &str, advertise: &[String]) -> Result<PackFile> {
    let mut body = Vec::new();
    for reference in advertise {
        let line = format!("want {reference}\n");
        let size = (line.len() as u16 + 4).to_be_bytes();
        write!(body, "{}{}", hex::encode(size), line)?;
    }
    body.extend("0000".as_bytes());
    body.extend("0009done\n".as_bytes());

    let url = format!("{url}/git-upload-pack");
    let client = Client::new();

    let mut res = client
        .post(&url)
        .header("Content-Type", "x-git-upload-pack-request")
        .body(body)
        .send()
        .await?
        .bytes()
        .await?;

    let _ = res.split_to(8);

    let pack = PackFile::new(res)?;
    Ok(pack)
}

fn build_repository(obj_ref: &str) -> Result<()> {
    let root_path = env::current_dir()?;
    build_repository_recursive(obj_ref, &root_path)
}

fn build_repository_recursive(obj_ref: &str, path: &Path) -> Result<()> {
    let obj = GitObject::from_ref(obj_ref)?;

    match obj.object_content() {
        ObjectContent::Tree { entries } => {
            for entry in entries {
                let entry_path = path.join(entry.filename());
                match entry.mode() {
                    "100755" => {
                        let _f = OpenOptions::new()
                            .create(true)
                            .write(true)
                            .truncate(true)
                            .mode(0o755)
                            .open(&entry_path)?;
                    }
                    "100644" => {
                        let _f = OpenOptions::new()
                            .create(true)
                            .write(true)
                            .truncate(true)
                            .mode(0o644)
                            .open(&entry_path)?;
                    }
                    "40000" => {
                        fs::create_dir_all(&entry_path)?;
                    }
                    _ => anyhow::bail!("unknow file permission"),
                };
                build_repository_recursive(&hex::encode(entry.hash()), &entry_path)?;
            }
        }
        ObjectContent::Blob { blob_content } => {
            fs::write(path, blob_content)?;
        }
        ObjectContent::Commit {
            tree_sha,
            parent: _,
            author: _,
            committer: _,
            message: _,
        } => {
            build_repository_recursive(tree_sha, path)?;
        }
        _ => {}
    };
    Ok(())
}
