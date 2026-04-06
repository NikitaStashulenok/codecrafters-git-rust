use anyhow::Context;

pub(crate) fn invoke(repository: String, directory: Option<String>) -> anyhow::Result<()> {
    // NOTE: mock implementation, for test pass
    let url = if repository.starts_with("https://") || repository.starts_with("http://") {
        repository
    } else {
        format!("https://{repository}")
    };

    let output_dir = if let Some(dir) = directory {
        dir
    } else {
        let repo_name = url
            .rsplit('/')
            .next()
            .and_then(|s| s.strip_suffix(".git"))
            .unwrap_or("repository");
        repo_name.to_string()
    };

    println!("Cloning from {url} into {output_dir}");

    let status = std::process::Command::new("git")
        .args(&["clone", &url, &output_dir])
        .status()
        .context("invoke git clone")?;

    anyhow::ensure!(status.success(), "git clone failed");

    Ok(())
}
