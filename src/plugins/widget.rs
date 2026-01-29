use crate::plugins::{PluginHost, PluginRegistry, PluginType};
use anyhow::Result;
use std::path::{Path, PathBuf};

const WIDGET_MAX_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone)]
pub struct PluginWidgetState {
    pub plugin_id: String,
    pub payload: Option<String>,
    pub error: Option<String>,
}

pub fn collect_channel_widgets(project_root: &Path) -> Result<Vec<PluginWidgetState>> {
    let registry = PluginRegistry::new()?;
    let mut host = PluginHost::new()?;
    let mut widgets = Vec::new();

    for plugin in registry.by_type(PluginType::Channel) {
        if !plugin.enabled {
            continue;
        }

        if !load_plugin_with_data_dir(&mut host, plugin, project_root) {
            widgets.push(PluginWidgetState {
                plugin_id: plugin.id().to_string(),
                payload: None,
                error: Some("load_failed".to_string()),
            });
            continue;
        }

        let Some(instance) = host.get_mut(plugin.id()) else {
            continue;
        };

        if !instance.has_channel_widget_state() {
            continue;
        }

        match instance.channel_widget_state() {
            Ok(payload) => {
                if payload.len() > WIDGET_MAX_BYTES {
                    widgets.push(PluginWidgetState {
                        plugin_id: plugin.id().to_string(),
                        payload: None,
                        error: Some("widget_too_large".to_string()),
                    });
                } else {
                    widgets.push(PluginWidgetState {
                        plugin_id: plugin.id().to_string(),
                        payload: Some(payload),
                        error: None,
                    });
                }
            }
            Err(err) => {
                widgets.push(PluginWidgetState {
                    plugin_id: plugin.id().to_string(),
                    payload: None,
                    error: Some(err.to_string()),
                });
            }
        }
    }

    Ok(widgets)
}

fn load_plugin_with_data_dir(
    host: &mut PluginHost,
    plugin: &crate::plugins::InstalledPlugin,
    project_root: &Path,
) -> bool {
    let data_dir = project_root.join("plugins").join(plugin.id()).join("data");
    let result = if data_dir.exists() {
        host.load_with_data_dir(plugin, data_dir)
    } else {
        host.load(plugin)
    };
    result.is_ok()
}
