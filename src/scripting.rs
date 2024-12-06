// scripting.rs
use std::fs::{self, File};
use std::path::Path;
use rand::Rng;
use reqwest::blocking::Client;
use zip::read::ZipArchive;
use tempfile::Builder;
use std::io::Write;
use wait_timeout::ChildExt;
use std::io::Read;
use std::sync::Once;
use crate::agent_config::{AgentConfig, ScriptConfig, get_install_dir_from_registry};
use std::sync::Arc;

static INIT: Once = Once::new();

pub(crate) fn setup_test_environment() -> std::io::Result<()> {
    // 使用临时目录进行测试
    let temp_dir = tempfile::Builder::new()
        .prefix("tactical-test")
        .tempdir()?;
    let base_dir = temp_dir.path();
    // 注释问题
    let config = AgentConfig {
        script: ScriptConfig {
            program_dir: base_dir.to_string_lossy().to_string(),
            py_bin: base_dir.join("runtime/python/python.exe").to_string_lossy().to_string(),
            nu_bin: base_dir.join("bin/nu.exe").to_string_lossy().to_string(),
            deno_bin: base_dir.join("bin/deno.exe").to_string_lossy().to_string(),
            // temp路径问题
            win_tmp_dir: base_dir.join("temp").to_string_lossy().to_string(),
            win_run_as_user_tmp_dir: base_dir.join("temp/user").to_string_lossy().to_string(),
            proxy: None,
        },
        ..AgentConfig::default()
    };

    // 创建目录结构
    // *******路径问题
    fs::create_dir_all(Path::new(&config.script.program_dir).join("runtime/python"))?;
    fs::create_dir_all(Path::new(&config.script.program_dir).join("bin"))?;
    fs::create_dir_all(&config.script.win_tmp_dir)?;
    fs::create_dir_all(&config.script.win_run_as_user_tmp_dir)?;
    
    Ok(())
}

fn setup() {
    INIT.call_once(|| {
        setup_test_environment().expect("Failed to setup test environment");
    });
}

pub struct ScriptExecutor {
    config: Arc<ScriptConfig>,
}

impl ScriptExecutor {
    pub fn new(config: AgentConfig) -> Self {
        Self {
            config: Arc::new(config.script),
        }
    }

    pub fn get_python(&self, force: bool) -> Result<(), String> {  // 修改返回类型
        if Path::new(&self.config.py_bin).exists() && !force {
            return Ok(());  // 如果已存在，直接返回成功
        }

        if force {
            if let Some(parent) = Path::new(&self.config.py_bin).parent() {
                fs::remove_dir_all(parent).map_err(|e| format!("Failed to remove directory: {}", e))?;
            }
        }

        let sleep_delay = rand::thread_rng().gen_range(1..=10);
        println!("GetPython() sleeping for {} seconds", sleep_delay);
        std::thread::sleep(std::time::Duration::new(sleep_delay as u64, 0));

        // 确保父目录存在
        if let Some(parent) = Path::new(&self.config.py_bin).parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Failed to create base directory: {}", e))?;
        }

        let arch_zip = "py3.11.9_amd64.zip";
        let py_zip = Path::new(&self.config.py_bin).parent().unwrap().join(arch_zip);

        let py_zip_clone = py_zip.clone();
        let _cleanup = std::panic::catch_unwind(move || {
            fs::remove_file(&py_zip_clone).ok();
        });
// ******Client::builder重复率较高
        let client = Client::builder();

        // 配置代理
        let client = if let Some(proxy_url) = &self.config.proxy {
            println!("使用代理: {}", proxy_url);
            client.proxy(reqwest::Proxy::all(proxy_url)
                .map_err(|e| format!("代理设置无效: {}", e))?)
        } else {
            client
        };

        let client = client.build().map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;

        let url = "https://github.com/amidaware/rmmagent/releases/download/v2.8.0/py3.11.9_amd64.zip";
        println!("Downloading from URL: {}", url);

        let mut response = client.get(url)
            .send()
            .map_err(|e| format!("无法下载 py3.11.9_amd64.zip: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("Unable to download py3.11.9_amd64.zip from GitHub. Status code: {}", response.status()));
        }

        let mut file = File::create(&py_zip).map_err(|e| format!("Failed to create zip file: {}", e))?;
        response.copy_to(&mut file).map_err(|e| format!("Failed to save zip file: {}", e))?;

        // 解压前确保每个子目录都存在
        if let Err(err) = self.unzip(&py_zip, &self.config.py_bin) {
            return Err(format!("解压失败: {}", err));
        }

        Ok(())  // 返回成功
    }

    fn file_exists(&self, path: &str) -> bool {
        Path::new(path).exists()
    }

    fn unzip(&self, zip_path: &Path, dest_dir: &str) -> Result<(), String> {
        let file = File::open(zip_path).map_err(|e| format!("Failed to open zip file: {}", e))?;
        let mut archive = ZipArchive::new(file).map_err(|e| format!("Failed to read zip archive: {}", e))?;

        for i in 0..archive.len() {
            let mut file = archive.by_index(i).map_err(|e| format!("Failed to read file from zip: {}", e))?;
            let out_path = Path::new(dest_dir).join(file.name());

            // 确保解压时每个子目录都存在，防止创建文件时路径不存在
            if let Some(parent_dir) = out_path.parent() {
                if !parent_dir.exists() {
                    fs::create_dir_all(parent_dir).map_err(|e| format!("Failed to create directory: {}", e))?;
                }
            }

            if file.name().ends_with('/') {
                // 是目录项，创建目录
                fs::create_dir_all(&out_path).map_err(|e| format!("Failed to create dir: {}", e))?;
            } else {
                // 是文件项，写入文件
                let mut output_file = File::create(&out_path)
                    .map_err(|e| format!("Failed to create file: {}", e))?;
                std::io::copy(&mut file, &mut output_file)
                    .map_err(|e| format!("Failed to extract file: {}", e))?;
            }
        }

        Ok(())
    }
    pub fn install_nu_shell(&self, force: bool) -> Result<(), String> {
        if Path::new(&self.config.nu_bin).exists() && !force {
            return Ok(());
        }

        if force && self.file_exists(&self.config.nu_bin) {
            fs::remove_file(&self.config.nu_bin)
                .map_err(|e| format!("Error removing nu.exe binary: {}", e))?;
        }

        // 随机延迟
        let sleep_delay = rand::thread_rng().gen_range(1..=10);
        println!("InstallNuShell() sleeping for {} seconds", sleep_delay);
        std::thread::sleep(std::time::Duration::new(sleep_delay as u64, 0));

        // 创建程序目录
        let program_bin_dir = Path::new(&self.config.program_dir).join("bin");
        if !program_bin_dir.exists() {
            fs::create_dir_all(&program_bin_dir)
                .map_err(|e| format!("Error creating Program Files bin folder: {}", e))?;
        }

        // 创建配置目录和文件
        let nu_shell_path = Path::new(&self.config.program_dir).join("etc").join("nu_shell");
        let nu_shell_config = nu_shell_path.join("config.nu");
        let nu_shell_env = nu_shell_path.join("env.nu");

        if !nu_shell_path.exists() {
            fs::create_dir_all(&nu_shell_path)
                .map_err(|e| format!("Error creating nu_shell config directory: {}", e))?;
        }

        // 创建配置文件
        for config_file in &[nu_shell_config, nu_shell_env] {
            if !config_file.exists() {
                File::create(config_file)
                    .map_err(|e| format!("Error creating config file: {}", e))?;
                #[cfg(unix)]
                std::fs::set_permissions(config_file, std::fs::Permissions::from_mode(0o744))
                    .map_err(|e| format!("Error setting permissions: {}", e))?;
            }
        }

        // 下载 URL 构建
        let version = "0.87.0"; // 可以设置为配置项
        let asset_name = if cfg!(target_os = "windows") {
            match () {
                _ if cfg!(target_arch = "x86_64") => {
                    format!("nu-{}-x86_64-windows-msvc-full.zip", version)
                }
                _ if cfg!(target_arch = "aarch64") => {
                    format!("nu-{}-arm64-windows-msvc-full.zip", version)
                }
                _ => return Err("Unsupported architecture. Only x86_64 and aarch64 are supported.".to_string()),
            }
        } else {
            return Err("Unsupported OS. Only Windows is supported.".to_string());
        };

        let url = format!("https://github.com/nushell/nushell/releases/download/{}/{}", version, asset_name);
        println!("Nu download url: {}", url);

        // 创建临时目录
        let tmp_dir = tempfile::Builder::new()
            .prefix("nu_temp")
            .tempdir()
            .map_err(|e| format!("Error creating temp directory: {}", e))?;

        // 下载文件
        let client = Client::builder();
        let client = if let Some(proxy_url) = &self.config.proxy {
            client.proxy(reqwest::Proxy::all(proxy_url)
                .map_err(|e| format!("Invalid proxy settings: {}", e))?)
        } else {
            client
        };
        let client = client.build()
            .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

        let tmp_asset_path = tmp_dir.path().join(&asset_name);
        let mut response = client.get(&url)
            .send()
            .map_err(|e| format!("Failed to download nu shell: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("Download failed with status: {}", response.status()));
        }

        let mut file = File::create(&tmp_asset_path)
            .map_err(|e| format!("Failed to create temp file: {}", e))?;
        response.copy_to(&mut file)
            .map_err(|e| format!("Failed to save downloaded file: {}", e))?;

        // 解压并安装
        self.unzip(&tmp_asset_path, tmp_dir.path().to_str().unwrap())?;

        // 检查解压后的文件是否存在
        let nu_exe_path = tmp_dir.path().join("nu.exe");

        println!("解压后的路径: {:?}", nu_exe_path);
        if !nu_exe_path.exists() {
            return Err(format!("解压失败：nu.exe 未能解压到临时目录: {:?}", nu_exe_path));
        }

        // 再次检查并打印目标路径
        println!("准备复制 nu.exe 到目标路径: {:?}", self.config.nu_bin);

        // 创建目标路径的目录
        let target_dir = Path::new(&self.config.nu_bin).parent().unwrap();
        if !target_dir.exists() {
            fs::create_dir_all(target_dir)
                .map_err(|e| format!("Error creating target directory: {}", e))?;
        }

        // 复制可执行文件
        fs::copy(
            &nu_exe_path,
            &self.config.nu_bin
        ).map_err(|e| format!("Failed to copy nu.exe: {}", e))?;
        println!("nu.exe 成功复制到目标路径: {:?}", self.config.nu_bin);
        Ok(())
    }

    pub fn install_deno(&self, force: bool) -> Result<(), String> {
        if Path::new(&self.config.deno_bin).exists() && !force {
            return Ok(());
        }

        if force && self.file_exists(&self.config.deno_bin) {
            fs::remove_file(&self.config.deno_bin)
                .map_err(|e| format!("删除 deno.exe 失败: {}", e))?;
        }

        let sleep_delay = rand::thread_rng().gen_range(1..=10);
        println!("InstallDeno() sleeping for {} seconds", sleep_delay);
        std::thread::sleep(std::time::Duration::new(sleep_delay as u64, 0));

        // 创建临时目录
        let tmp_dir = tempfile::Builder::new()
            .prefix("tactical-deno")
            .tempdir()
            .map_err(|e| format!("创建临时目录失败: {}", e))?;

        let asset_name = "deno-x86_64-pc-windows-msvc.zip";
        let tmp_asset_path = tmp_dir.path().join(asset_name);

        // 修改客户端配置，添加超时设置
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(60)) // 添加60秒超时
            .connect_timeout(std::time::Duration::from_secs(30)); // 添加30秒连接超时

        let client = if let Some(proxy_url) = &self.config.proxy {
            client
                .proxy(reqwest::Proxy::all(proxy_url)
                    .map_err(|e| format!("代理配置失败: {}", e))?)
                .build()
                .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?
        } else {
            client
                .build()
                .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?
        };

        // 下载 Deno
        let url = "https://github.com/denoland/deno/releases/download/v1.40.2/deno-x86_64-pc-windows-msvc.zip";
        println!("Deno download url: {}", url);

        let mut response = client.get(url)
            .send()
            .map_err(|e| format!("下载失败: {}", e))?;
        let mut file = File::create(&tmp_asset_path)
            .map_err(|e| format!("创建临时文件失败: {}", e))?;
        response.copy_to(&mut file)
            .map_err(|e| format!("保存下载文件失败: {}", e))?;

        // 解压并安装
        self.unzip(&tmp_asset_path, tmp_dir.path().to_str().unwrap())?;

        // 确保目标目录存在
        if let Some(parent) = Path::new(&self.config.deno_bin).parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("创建目标目录失败: {}", e))?;
        }

        // 复制可执行文件
        fs::copy(
            tmp_dir.path().join("deno.exe"),
            &self.config.deno_bin
        ).map_err(|e| format!("复制 deno.exe 失败: {}", e))?;

        Ok(())
    }

    pub fn run_script(
        &self,
        code: &str,
        shell: &str,
        args: Vec<String>,
        timeout: i32,
        run_as_user: bool,
        env_vars: Vec<String>,
        nushell_enable_config: bool,
        deno_default_permissions: &str,
    ) -> Result<(String, String, i32), String> {
        // 首先检查脚本环境
        self.check_script_environment(shell)?;

        let tmp_dir = if run_as_user {
            &self.config.win_run_as_user_tmp_dir
        } else {
            &self.config.win_tmp_dir
        };

        // 确保临时目录存在
        if !Path::new(tmp_dir).exists() {
            fs::create_dir_all(tmp_dir)
                .map_err(|e| format!("创建临时目录失败: {}", e))?;
        }
        // 1. 获取文件扩展名
        let extension = match shell {
            "powershell" => ".ps1",
            "python" => ".py",
            "cmd" => ".bat",
            "nushell" => ".nu",
            "deno" => ".ts",
            _ => return Err(format!("不支持的脚本类型: {}", shell)),
        };

        // 2. 创建临时文件
        let temp_file = Builder::new()
            .prefix("script_")
            .suffix(extension)
            .tempfile_in(tmp_dir)
            .map_err(|e| format!("创建临时文件失败: {}", e))?;

        // 3. 写入脚本内容
        temp_file.as_file()
            .write_all(code.as_bytes())
            .map_err(|e| format!("写入脚本内容失败: {}", e))?;

        // 4. 转为 PathBuf 并返回
        let script_path = temp_file.into_temp_path();

        // 创建一个绑定来延长临时值的生命周期
        let path_string = script_path.to_string_lossy();
        
        let (exe, mut cmd_args) = match shell {
            "powershell" => (
                "powershell.exe",
                vec![
                    "-NonInteractive".to_string(),
                    "-NoProfile".to_string(),
                    "-ExecutionPolicy".to_string(),
                    "Bypass".to_string(),
                    path_string.to_string(),
                ]
            ),
            "python" => (
                self.config.py_bin.as_str(),
                vec![path_string.to_string()]
            ),
            "cmd" => (
                path_string.as_ref(),
                Vec::new()
            ),
            "nushell" => {
                if !Path::new(&self.config.nu_bin).exists() {
                    return Err("Nushell executable not found".to_string());
                }
                let mut nushell_args = if nushell_enable_config {
                    vec![
                        "--config".to_string(),
                        format!("{}/etc/nushell/config.nu", self.config.program_dir),
                        "--env-config".to_string(),
                        format!("{}/etc/nushell/env.nu", self.config.program_dir),
                    ]
                } else {
                    vec!["--no-config-file".to_string()]
                };
                nushell_args.push(path_string.to_string());
                (self.config.nu_bin.as_str(), nushell_args)
            },
            "deno" => {
                if !Path::new(&self.config.deno_bin).exists() {
                    return Err("Deno executable not found".to_string());
                }
                let mut deno_args = vec!["run".to_string(), "--no-prompt".to_string()];
                
                // 处理 Deno 权限
                let mut found = false;
                for env_var in &env_vars {
                    if env_var.starts_with("DENO_PERMISSIONS=") {
                        if let Some(permissions) = env_var.split('=').nth(1) {
                            deno_args.extend(permissions.split_whitespace().map(String::from));
                            found = true;
                            break;
                        }
                    }
                }
                
                if !found && !deno_default_permissions.is_empty() {
                    deno_args.extend(deno_default_permissions.split_whitespace().map(String::from));
                }
                
                deno_args.push(path_string.to_string());
                (self.config.deno_bin.as_str(), deno_args)
            },
            _ => return Err(format!("Unsupported shell type: {}", shell)),
        };

        // 加额外参数
        cmd_args.extend(args);

        println!("exe:{}",exe);
        // 创建命令
        let mut cmd = std::process::Command::new(exe);
        cmd.args(&cmd_args);

        // 设置环境变量
        if !env_vars.is_empty() {
            cmd.envs(env_vars.iter().filter_map(|var| {
                let parts: Vec<&str> = var.split('=').collect();
                if parts.len() == 2 {
                    Some((parts[0], parts[1]))
                } else {
                    None
                }
            }));
        }

        // 创建管道
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        // 启动进程
        let mut child = cmd.spawn()
            .map_err(|e| format!("Failed to start process: {}", e))?;

        // 设置超时
        let timeout = std::time::Duration::from_secs(timeout as u64);

        // 获取输出
        let output = match child.wait_timeout(timeout)
            .map_err(|e| format!("Error waiting for process: {}", e))? {
            Some(status) => {
                let stdout = String::from_utf8_lossy(&child.stdout.take()
                    .ok_or("Failed to capture stdout")?
                    .bytes()
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| format!("Failed to read stdout: {}", e))?).to_string();
                
                let stderr = String::from_utf8_lossy(&child.stderr.take()
                    .ok_or("Failed to capture stderr")?
                    .bytes()
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| format!("Failed to read stderr: {}", e))?).to_string();

                let exit_code = status.code().unwrap_or(1);
                Ok::<(String, String, i32), String>((stdout, stderr, exit_code))
            },
            None => {
                // 超时处理
                child.kill()
                    .map_err(|e| format!("Failed to kill process: {}", e))?;
                
                Ok::<(String, String, i32), String>((
                    String::new(),
                    format!("Script timed out after {} seconds", timeout.as_secs()),
                    98
                ))
            }
        }?;

        Ok(output)
    }

    // 添加新的辅助方法来检查脚本环境
    fn check_script_environment(&self, shell: &str) -> Result<(), String> {
        match shell {
            "python" => {
                if !self.file_exists(&self.config.py_bin) {
                    return Err("Python 未安装，请先运行 get_python()".to_string());
                }
            },
            "powershell" => {
                // Windows 系统通常预装 PowerShell，但仍可以检查
                if !Path::new("powershell.exe").exists() && 
                   !Path::new("C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe").exists() {
                    return Err("PowerShell 未找到".to_string());
                }
            },
            "cmd" => {
                // Windows 系统预装 CMD，但仍可以检查
                if !Path::new("cmd.exe").exists() && 
                   !Path::new("C:\\Windows\\System32\\cmd.exe").exists() {
                    return Err("CMD 未找到".to_string());
                }
            },
            "nushell" => {
                if !self.file_exists(&self.config.nu_bin) {
                    return Err("Nushell 未安装，请先运行 install_nu_shell()".to_string());
                }
            },
            "deno" => {
                if !self.file_exists(&self.config.deno_bin) {
                    return Err("Deno 未安装，请先运行 install_deno()".to_string());
                }
            },
            _ => return Err(format!("不支持的脚本类型: {}", shell)),
        }
        Ok(())
    }
}

// 测试代码
#[cfg(test)]
mod tests {
    use super::*;
    // 确保环境安装的辅助函数
    fn ensure_environments(executor: &ScriptExecutor) -> Result<(), String> {
        println!("正在检查并安装必要的脚本环境...");

        // 检查并安装 Python
        if !Path::new(&executor.config.py_bin).exists() {
            println!("安装 Python...");
            executor.get_python(false).map_err(|e| format!("python安装失败：{}", e))?;
        }

        // 检查并安装 Nushell
        if !Path::new(&executor.config.nu_bin).exists() {
            println!("安装 Nushell...");
            executor.install_nu_shell(false).map_err(|e| format!("Nushell安装失败：{}", e))?;
        }

        // 检查并安装 Deno
        if !Path::new(&executor.config.deno_bin).exists() {
            println!("安装 Deno...");
            executor.install_deno(false).map_err(|e| format!("Deno安装失败：{}", e))?;
        }

        println!("所有环境检查完成");
        Ok(())
    }

    fn create_test_executor() -> ScriptExecutor {
        let config = AgentConfig::default();
        let executor = ScriptExecutor::new(config);

        // 确保环境已安装
        ensure_environments(&executor)
            .expect("Failed to setup script environments");

        executor
    }
    // 添加 Clone 和 Debug traits
    #[derive(Clone, Debug)]
    struct ScriptTest {
        shell: &'static str,
        script: &'static str,
        expected_output: &'static str,
        args: Vec<String>,
        env_vars: Vec<String>,
        install_fn: fn(&ScriptExecutor) -> Result<(), String>,
    }
    fn test_script_execution(test_case: ScriptTest) -> Result<(), String> {
        let executor = create_test_executor();

        // 1. 安装/准备环境
        println!("=== 测试 {} 环境安装 ===", test_case.shell);
        (test_case.install_fn)(&executor)?;

        // 2. 执行脚本测试
        println!("=== 测试 {} 脚本执行 ===", test_case.shell);
        let (stdout, stderr, exit_code) = executor.run_script(
            test_case.script,
            test_case.shell,
            test_case.args,
            30,
            false,
            test_case.env_vars,
            false,
            "",
        )?;
        // 3. 验证结果
        println!("{} 输出: {}", test_case.shell, stdout);
        println!("{} 错误: {}", test_case.shell, stderr);
        println!("退出码: {}", exit_code);

        assert!(stdout.contains(test_case.expected_output),
                "未找到预期输出: {}", test_case.expected_output);
        assert_eq!(exit_code, 0, "脚本执行失败");
        Ok(())
    }
    fn cleanup_test_environment() -> std::io::Result<()> {
        if let Some(install_dir) = get_install_dir_from_registry() {
            // 清理临时目录
            let temp_dir = install_dir.join("temp");
            let temp_user_dir = install_dir.join("temp/user");

            if temp_dir.exists() {
                fs::remove_dir_all(&temp_dir).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
            }

            if temp_user_dir.exists() {
                fs::remove_dir_all(&temp_user_dir).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
            }

            // 清理 Python、Nushell、Deno 相关目录
            let py_dir = install_dir.join("runtime/python");
            let nu_dir = install_dir.join("runtime/nushell");
            let deno_dir = install_dir.join("runtime/deno");

            if py_dir.exists() {
                fs::remove_dir_all(py_dir).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
            }

            if nu_dir.exists() {
                fs::remove_dir_all(nu_dir).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
            }

            if deno_dir.exists() {
                fs::remove_dir_all(deno_dir).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
            }
        }

        Ok(())
    }
    //*******"C:\Program Files\nextrmm-agent\runtime\python\python.exe\py3.11.9_amd64\python.exe"
    #[test]
    fn test_all_shells() {
        cleanup_test_environment().expect("清理临时目录失败");
        setup();
        // Python 测试用例
        let python_test = ScriptTest {
            shell: "python",
            script: "print('Hello from Python!')",
            expected_output: "Hello from Python!",
            args: vec![],
            env_vars: vec![],
            install_fn: |executor| {
                // 现在这里只需要检查环境是否存在
                if !Path::new(&executor.config.py_bin).exists() {
                    executor.get_python(false).map_err(|e| format!("python安装失败：{}", e))?;
                }
                Ok(())
            },
        };

        // Nushell 测试用例
        let nushell_test = ScriptTest {
            shell: "nushell",
            script: "echo 'Hello from Nushell!'",
            expected_output: "Hello from Nushell!",
            args: vec![],
            env_vars: vec![],
            install_fn: |executor| {
                if !Path::new(&executor.config.nu_bin).exists() {
                    executor.install_nu_shell(false).map_err(|e| format!("Nushell安装失败：{}", e))?;
                }
                Ok(())
            },
        };

        // Deno 测试用例
        let deno_test = ScriptTest {
            shell: "deno",
            script: "console.log('Hello from Deno!')",
            expected_output: "Hello from Deno!",
            args: vec![],
            env_vars: vec![],
            install_fn: |executor| {
                if !Path::new(&executor.config.deno_bin).exists() {
                    executor.install_deno(false).map_err(|e| format!("Deno安装失败：{}", e))?;
                }
                Ok(())
            },
        };
        // 执行所有测试
        for test in [python_test, nushell_test, deno_test] {
            test_script_execution(test.clone())
                .unwrap_or_else(|e| panic!("{} 测试失败: {}", test.shell, e));
        }
    }
    #[test]
    fn test_timeout_and_error_cases() {
        cleanup_test_environment().expect("清理临时目录失败");
        let executor = create_test_executor();

        // 测试超时情况
        let timeout_result = executor.run_script(
            "import time\ntime.sleep(5)",
            "python",
            vec![],
            2,  // 2秒超时
            false,
            vec![],
            false,
            "",
        );

        match timeout_result {
            Ok((_, stderr, exit_code)) => {
                assert!(stderr.contains("timed out"));
                assert_eq!(exit_code, 98);
            },
            Err(e) => panic!("超时测试执行失败: {}", e),
        }

        // 测试无效的脚本类型
        let invalid_shell_result = executor.run_script(
            "echo 'test'",
            "invalid_shell",
            vec![],
            30,
            false,
            vec![],
            false,
            "",
        );

        assert!(invalid_shell_result.is_err());
        assert!(invalid_shell_result.unwrap_err().contains("不支持的脚本类型"));
    }

    #[test]
    fn test_environment_variables() {
        cleanup_test_environment().expect("清理临时目录失败");
        let executor = create_test_executor();

        // 首先确保 Deno 已安装
        executor.install_deno(false).expect("安装 Deno 失败");

        let test_cases = vec![
            ("python", "import os\nprint(os.getenv('TEST_VAR'))", "test_value", vec![]),
            ("nushell", "echo $env.TEST_VAR", "test_value", vec![]),
            ("deno", "console.log(Deno.env.get('TEST_VAR'))", "test_value",
             vec!["--allow-env".to_string()]),  // 简化 Deno 命令参数
        ];

        for (shell, script, expected, args) in test_cases {
            println!("测试 {} 环境变量", shell);  // 添加调试信息

            let result = executor.run_script(
                script,
                shell,
                args,
                30,
                false,
                vec!["TEST_VAR=test_value".to_string()],
                false,
                "--allow-env",  // 添加默认权限
            );

            match result {
                Ok((stdout, stderr, exit_code)) => {
                    // 添加更多调试信息
                    println!("{}测试结果:", shell);
                    println!("stdout: '{}'", stdout);
                    println!("stderr: '{}'", stderr);
                    println!("exit_code: {}", exit_code);

                    assert!(stdout.contains(expected),
                            "{} 环境变量测试失败: 预期 '{}', 实际输出 '{}'",
                            shell, expected, stdout);
                    assert_eq!(exit_code, 0);
                },
                Err(e) => panic!("{} 环境变量测试失败: {}", shell, e),
            }
        }
    }
}



