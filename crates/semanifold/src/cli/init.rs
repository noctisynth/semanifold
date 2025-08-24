use std::{fmt::Debug, path::PathBuf};

use clap::{Args, arg};
use log::info;
use semanifold_resolver::analyzer;

#[derive(Debug, Args)]
pub(crate) struct Init {
    #[arg(short, long, default_value = "./.changes")]
    pub root: PathBuf,
}

pub(crate) fn run(init: &Init) -> anyhow::Result<()> {
    std::fs::create_dir_all(&init.root)?;

    let config_path = init.root.join("config.toml");

    let parent_path = init.root.parent().unwrap().to_path_buf();
    //init config
    let analyzer = analyzer::default(&parent_path)?;

    let config = analyzer.analyze()?;

    // 写入config.toml文件
    let config_doc = toml::to_string_pretty(&config)?;

    // write config.toml file
    std::fs::write(config_path, config_doc)?;

    info!("Initialized semanifold in {}", init.root.display());
    Ok(())
}
