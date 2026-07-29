use anyhow::Result;
use console::style;

pub fn handle_plugin_install(url: &str, auto_update: bool) -> Result<()> {
    let install = goose::plugins::install_plugin_with_options(
        url,
        goose::plugins::PluginInstallOptions { auto_update },
    )?;

    println!(
        "{} Installed {} plugin '{}' ({})",
        style("✓").green(),
        install.format,
        style(&install.name).bold(),
        install.version
    );
    print_plugin_install(&install);

    Ok(())
}

pub fn handle_plugin_update(name: &str) -> Result<()> {
    let install = goose::plugins::update_plugin(name)?;

    println!(
        "{} Updated {} plugin '{}' ({})",
        style("✓").green(),
        install.format,
        style(&install.name).bold(),
        install.version
    );
    print_plugin_install(&install);

    Ok(())
}

fn print_plugin_install(install: &goose::plugins::PluginInstall) {
    println!("  Source: {}", install.source);
    println!("  Location: {}", install.directory.display());

    if install.skills.is_empty() {
        println!("  No skills imported.");
    } else {
        println!("  Imported skills:");
        for skill in &install.skills {
            println!("    - {}", skill.name);
        }
    }
}

pub fn handle_plugin_list() -> Result<()> {
    let plugins = goose::plugins::installed_plugins()?;
    if plugins.is_empty() {
        println!("No plugins installed.");
        return Ok(());
    }
    for plugin in plugins {
        println!("{}  {}", plugin.name, plugin.source);
    }
    Ok(())
}

pub fn handle_plugin_uninstall(name: &str) -> Result<()> {
    goose::plugins::uninstall_plugin(name)?;
    println!("{} Removed plugin '{}'", style("✓").green(), name);
    Ok(())
}

pub fn handle_marketplace_add(source: &str) -> Result<()> {
    let (name, record) = goose::plugins::marketplace_add(source)?;
    println!(
        "{} Added marketplace '{}' ({} plugins)",
        style("✓").green(),
        name,
        record.plugin_count
    );
    println!("  Source: {}", record.source);
    Ok(())
}

pub fn handle_marketplace_list(with_plugins: bool) -> Result<()> {
    let registered = goose::marketplaces::registered();
    if registered.is_empty() {
        println!("No marketplaces registered. Built-in defaults are still searched.");
    }
    for (name, record) in &registered {
        println!(
            "{}  {} ({} plugins)",
            name, record.source, record.plugin_count
        );
    }

    if with_plugins {
        for (name, marketplace) in goose::plugins::marketplace_plugins(None)? {
            println!("\n{} ({} plugins)", name, marketplace.plugins.len());
            for entry in &marketplace.plugins {
                println!("  {}", entry.name);
            }
        }
    }
    Ok(())
}

pub fn handle_marketplace_remove(name: &str) -> Result<()> {
    if goose::plugins::marketplace_remove(name)? {
        println!("{} Removed marketplace '{}'", style("✓").green(), name);
    } else {
        println!("Marketplace '{}' was not registered.", name);
    }
    Ok(())
}

pub fn handle_marketplace_update(name: &str) -> Result<()> {
    let record = goose::plugins::marketplace_update(name)?;
    println!(
        "{} Updated marketplace '{}' ({} plugins)",
        style("✓").green(),
        name,
        record.plugin_count
    );
    Ok(())
}
