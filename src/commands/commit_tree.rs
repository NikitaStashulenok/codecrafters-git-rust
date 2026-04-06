use crate::objects::{Kind, Object};
use anyhow::Context;
use anyhow::Ok;
use std::fmt::Write;
use std::io::Cursor;

pub(crate) fn invoke(
    message: String,
    tree_hash: String,
    parent_hash: Option<String>,
) -> anyhow::Result<()> {
    // NOTE: the ?s here for write will never trigger as we're writing into a String
    let mut commit = String::new();
    writeln!(commit, "tree {tree_hash}")?;
    if let Some(parent_hash) = parent_hash {
        writeln!(commit, "parent {parent_hash}")?;
    }
    writeln!(
        commit,
        "author Nikita Stashulenok <nikita.stashulenok@mail.ru> 1775222706 +0300"
    )?;
    writeln!(
        commit,
        "committer Nikita Stashulenok <nikita.stashulenok@mail.ru> 1775222706 +0300"
    )?;
    writeln!(commit, "")?;
    writeln!(commit, "{message}")?;
    // commit.push_str(&message);

    let hash = Object {
        kind: Kind::Commit,
        expected_size: commit.len() as u64,
        reader: Cursor::new(commit),
    }
    .write_to_objects()
    .context("stream tree obj")?;

    println!("{}", hex::encode(hash));

    Ok(())
}
