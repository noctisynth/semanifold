pub fn run_async<F: Future>(f: F) -> anyhow::Result<()> {
    let rt = tokio::runtime::Runtime::new()?;

    rt.block_on(f);

    Ok(())
}
