use crate::objects::Object;
use anyhow::Context;
use std::path::PathBuf;

pub(crate) fn invoke(write: bool, file: PathBuf) -> anyhow::Result<()> {
    let object = Object::blob_from_file(file).context("open blob input file")?;
    let hash = if write {
        object
            .write_to_objects()
            .context("stream file into blob object file")?
    } else {
        object
            .write(std::io::sink())
            .context("stream file into blob object")?
    };

    println!("{}", hex::encode(hash));
    Ok(())
}
