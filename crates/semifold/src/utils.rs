pub fn run_async<F>(f: F) -> anyhow::Result<()>
where
    F: Future<Output = anyhow::Result<()>>,
{
    let rt = tokio::runtime::Runtime::new()?;

    rt.block_on(f)
}
