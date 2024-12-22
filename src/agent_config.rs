use std::fs;
use std::path::{Path, PathBuf};
use anyhow::Result;
use winreg::RegKey;
use winreg::enums::*;
use std::env;
#[derive(Debug, Clone)]
pub struct AgentConfig {
    // 基本配置
    pub agent_id: String,
    pub agent_version: String,

    // MQTT 配置
    pub mqtt: MqttConfig,

    // 脚本配置
    pub script: ScriptConfig,
}

#[derive(Debug, Clone)]
pub struct MqttConfig {
    pub broker_host: String,
    pub broker_port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub use_tls: bool,
    pub command_topic: String,      // 接收命令的主题
    pub keep_alive: u16,            // 心跳间隔（秒）
    pub reconnect_interval: u64,    // 重连间隔（秒）
}

#[derive(Debug, Clone)]
pub struct ScriptConfig {
    // 脚本运行时路径
    pub py_bin: String,
    pub nu_bin: String,
    pub deno_bin: String,

    // 工作目录
    pub program_dir: String,

    // 临时目录
    pub win_tmp_dir: String,
    pub win_run_as_user_tmp_dir: String,

    // 代理设置
    pub proxy: Option<String>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            agent_id: uuid::Uuid::new_v4().to_string(),
            agent_version: env!("CARGO_PKG_VERSION").to_string(),
            mqtt: MqttConfig {
                broker_host: "broker.emqx.io".to_string(),
                broker_port: 1883,
                username: None,
                password: None,
                use_tls: false,
                command_topic: format!("rmm/agent/{}/command", uuid::Uuid::new_v4()),
                keep_alive: 60,
                reconnect_interval: 5,
            },
            script: ScriptConfig::default(),
        }
    }
}

//Get installation directory from registry
pub fn get_install_dir_from_registry() -> Option<PathBuf> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    match hklm.open_subkey("SOFTWARE\\TacticalRMM") {
        Ok(key) => {
            match key.get_value::<String, _>("InstallDir") {
                Ok(install_dir) => Some(PathBuf::from(install_dir)),
                Err(_) => None
            }
        }
        Err(_) => None
    }
}

// Method 1: Get system drive using environment variable
fn get_system_drive_env() -> Option<PathBuf> {
    env::var("SystemDrive")
        .map(PathBuf::from)
        .ok()
}

// Method 2: Get system drive using Windows API
#[cfg(windows)]
fn get_system_drive_winapi() -> Option<PathBuf> {
    use windows::Win32::System::SystemInformation::GetWindowsDirectoryW;

    let mut buffer = [0u16; 260];
    let len = unsafe {
        GetWindowsDirectoryW(Some(&mut buffer))
    };

    if len > 0 {
        let path = String::from_utf16_lossy(&buffer[..len as usize]);
        PathBuf::from(path)
            .parent()
            .map(|p| p.to_path_buf())
    } else {
        None
    }
}

// Comprehensive method: Get system drive
fn get_system_drive() -> PathBuf {
    // Try method 1: Environment variable
    get_system_drive_env()
        // If method 1 fails, try method 2: Windows API
        .or_else(|| get_system_drive_winapi())
        // If both methods fail, use default value
        .unwrap_or_else(|| PathBuf::from("C:\\"))
}

fn get_program_dir() -> PathBuf {
    // Prioritize getting installation directory from registry
    get_install_dir_from_registry()
        // If registry is not found, use default system drive path
        .unwrap_or_else(|| {
            let system_drive = get_system_drive();
            system_drive.join("Program Files\\nextrmm-agent")
        })
}

impl Default for ScriptConfig {
    fn default() -> Self {
        let install_dir = get_program_dir();
        Self {
            program_dir: install_dir.to_string_lossy().to_string(),
            py_bin: install_dir.join("runtime\\python\\python.exe").to_string_lossy().to_string(),
            nu_bin: install_dir.join("runtime\\nushell\\nu.exe").to_string_lossy().to_string(),
            deno_bin: install_dir.join("runtime\\deno\\deno.exe").to_string_lossy().to_string(),
            win_tmp_dir: install_dir.join("temp").to_string_lossy().to_string(),
            win_run_as_user_tmp_dir: install_dir.join("temp\\user").to_string_lossy().to_string(),
            proxy: None,
        }
    }
}

impl AgentConfig {
    //  Load configuration from file
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        let lines: Vec<&str> = content.lines().collect();
        let mut config = AgentConfig::default();

        for line in lines {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let parts: Vec<&str> = line.splitn(2, '=').collect();
            if parts.len() != 2 {
                continue;
            }

            let key = parts[0].trim();
            let value = parts[1].trim();

            match key {
                "agent_id" => config.agent_id = value.to_string(),
                "agent_version" => config.agent_version = value.to_string(),
                "broker_host" => config.mqtt.broker_host = value.to_string(),
                "broker_port" => config.mqtt.broker_port = value.parse()?,
                "username" => config.mqtt.username = Some(value.to_string()),
                "password" => config.mqtt.password = Some(value.to_string()),
                "use_tls" => config.mqtt.use_tls = value.parse()?,
                "command_topic" => config.mqtt.command_topic = value.to_string(),
                "keep_alive" => config.mqtt.keep_alive = value.parse()?,
                "reconnect_interval" => config.mqtt.reconnect_interval = value.parse()?,
                _ => {}
            }
        }

        Ok(config)
    }

    // Save configuration to file
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let mut content = String::new();

        content.push_str(&format!("agent_id = {}\n", self.agent_id));
        content.push_str(&format!("agent_version = {}\n", self.agent_version));
        content.push_str(&format!("broker_host = {}\n", self.mqtt.broker_host));
        content.push_str(&format!("broker_port = {}\n", self.mqtt.broker_port));

        if let Some(username) = &self.mqtt.username {
            content.push_str(&format!("username = {}\n", username));
        }
        if let Some(password) = &self.mqtt.password {
            content.push_str(&format!("password = {}\n", password));
        }

        content.push_str(&format!("use_tls = {}\n", self.mqtt.use_tls));
        content.push_str(&format!("command_topic = {}\n", self.mqtt.command_topic));
        content.push_str(&format!("keep_alive = {}\n", self.mqtt.keep_alive));
        content.push_str(&format!("reconnect_interval = {}\n", self.mqtt.reconnect_interval));

        fs::write(path, content)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_config_save_load() {
        let config = AgentConfig::default();
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();

        // Save configuration
        config.save(path).unwrap();

        // Load configuration
        let loaded_config = AgentConfig::load(path).unwrap();

        // Verify configuration
        assert_eq!(config.agent_id, loaded_config.agent_id);
        assert_eq!(config.agent_version, loaded_config.agent_version);
        assert_eq!(config.mqtt.broker_host, loaded_config.mqtt.broker_host);
        assert_eq!(config.mqtt.broker_port, loaded_config.mqtt.broker_port);
    }

    #[test]
    fn test_default_config() {
        let config = AgentConfig::default();
        assert_eq!(config.mqtt.broker_port, 1883);
        assert_eq!(config.mqtt.keep_alive, 60);
        assert_eq!(config.mqtt.reconnect_interval, 5);
        assert_eq!(config.agent_version, env!("CARGO_PKG_VERSION"));
    }
}