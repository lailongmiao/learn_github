// scripting.rs
use std::fs::{self, File};
use std::path::Path;
use rand::Rng;
use reqwest::blocking::Client;
use zip::read::ZipArchive;
use tempfile::Builder;
use std::io::Write;
use wait_timeout::ChildExt;
use crate::agent_config::{AgentConfig, ScriptConfig, get_install_dir_from_registry};
use std::sync::Arc;
pub struct ScriptExecutor {
    config: Arc<ScriptConfig>,
}

impl ScriptExecutor {
    pub fn new(config: AgentConfig) -> Self {
        Self {
            config: Arc::new(config.script),
        }
    }

    pub fn get_python(&self, force: bool) -> Result<(), String> {
        // 如果已存在且不强制下载，则返回成功
        if Path::new(&self.config.py_bin).exists() && !force {
            return Ok(());
        }
        if force {
            if let Some(parent) = Path::new(&self.config.py_bin).parent() {
                fs::remove_dir_all(parent).map_err(|e| format!("Failed to remove directory: {}", e))?;
            }
        }

        let sleep_delay = rand::thread_rng().gen_range(1..=10);
        println!("GetPython() sleeping for {} seconds", sleep_delay);
        std::thread::sleep(std::time::Duration::new(sleep_delay as u64, 0));

        // 获取 runtime 目录，这里参考 py_bin 路径的父目录，但我们需要确保只获取到 "runtime" 而不是 "runtime\\python"
        let py_bin_path = Path::new(&self.config.py_bin);
        let runtime_dir = py_bin_path.parent()
            .and_then(|parent| parent.parent())  // 去掉 "python" 目录，获取到 "runtime" 目录
            .ok_or_else(|| "Failed to get runtime directory".to_string())?;

        // 创建 runtime 目录
        fs::create_dir_all(runtime_dir).map_err(|e| format!("Failed to create runtime directory: {}", e))?;

        // 解压后的目标目录是 runtime 目录
        let arch_zip = "py3.11.9_amd64.zip";
        let py_zip = runtime_dir.join(arch_zip);  // 将 ZIP 文件保存在 runtime 目录中

        // 处理 panic 时的清理工作
        let py_zip_clone = py_zip.clone();
        let _cleanup = std::panic::catch_unwind(move || {
            fs::remove_file(&py_zip_clone).ok();
        });

        // 创建 HTTP 客户端
        let client_builder = Client::builder();
        let client = if let Some(proxy_url) = &self.config.proxy {
            client_builder
                .proxy(reqwest::Proxy::all(proxy_url)
                    .map_err(|e| format!("代理配置失败: {}", e))?)
                .build()
                .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?
        } else {
            client_builder
                .build()
                .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?
        };

        let url = "https://github.com/amidaware/rmmagent/releases/download/v2.8.0/py3.11.9_amd64.zip";
        println!("Downloading from URL: {}", url);

        self.download_and_extract(&client, url, runtime_dir, "py3.11.9_amd64.zip", false)?;

        // 将解压后的目录重命名为 python（不再嵌套）
        let extracted_dir = runtime_dir.join("py3.11.9_amd64");  // 解压后的临时目录
        let final_dir = runtime_dir.join("python");  // 目标目录是 python

        // 如果目标目录已经存在，删除它（防止重命名失败）
        if final_dir.exists() {
            fs::remove_dir_all(&final_dir).map_err(|e| format!("Failed to remove existing python directory: {}", e))?;
        }

        // 将解压后的目录重命名为 python，解压后的文件将直接放在 runtime\\python
        if let Err(e) = fs::rename(extracted_dir, final_dir) {
            return Err(format!("Failed to rename extracted directory: {}", e));
        }

        // 最终应该是 runtime\\python\\python.exe
        Ok(())  // 返回成功
    }
    pub fn install_nu_shell(&self, force: bool) -> Result<(), String> {
        if Path::new(&self.config.nu_bin).exists() && !force {
            return Ok(());
        }

        if force && Path::new(&self.config.nu_bin).exists() {
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
            .tempdir()
            .map_err(|e| format!("Error creating temp directory: {}", e))?;

        // 创建 HTTP 客户端
        let client_builder = Client::builder();
        let client = if let Some(proxy_url) = &self.config.proxy {
            client_builder
                .proxy(reqwest::Proxy::all(proxy_url)
                    .map_err(|e| format!("代理配置失败: {}", e))?)
                .build()
                .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?
        } else {
            client_builder
                .build()
                .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?
        };

        // 下载文件
        self.download_and_extract(&client, &url, &program_bin_dir, &asset_name, true)?;

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

        if force && Path::new(&self.config.deno_bin).exists() {
            fs::remove_file(&self.config.deno_bin)
                .map_err(|e| format!("删除 deno.exe 失败: {}", e))?;
        }

        let sleep_delay = rand::thread_rng().gen_range(1..=10);
        println!("InstallDeno() sleeping for {} seconds", sleep_delay);
        std::thread::sleep(std::time::Duration::new(sleep_delay as u64, 0));

        // 创建临时目录
        let tmp_dir = tempfile::Builder::new()
            .tempdir()
            .map_err(|e| format!("创建临时目录失败: {}", e))?;

        let asset_name = "deno-x86_64-pc-windows-msvc.zip";
        let _tmp_asset_path = tmp_dir.path().join(asset_name);

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
        let url = "https://github.com/denoland/deno/releases/download/v2.1.3/deno-x86_64-pc-windows-msvc.zip";
        println!("Deno download url: {}", url);

        self.download_and_extract(&client, url, tmp_dir.path(), "deno-x86_64-pc-windows-msvc.zip", true)?;

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
    // 辅助方法：解压
    fn unzip(&self, zip_path: &Path, dest_dir: &str) -> Result<(), String> {
        let file = File::open(zip_path).map_err(|e| format!("Failed to open zip file: {}", e))?;
        let mut archive = ZipArchive::new(file).map_err(|e| format!("Failed to read zip archive: {}", e))?;

        for i in 0..archive.len() {
            let mut file = archive.by_index(i).map_err(|e| format!("Failed to read file from zip: {}", e))?;
            let out_path = Path::new(dest_dir).join(file.name());

            // 确保解压时每个子目录都存在
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

    pub fn run_script(
        &self,
        code: &str,
        shell: &str,
        args: Vec<String>,  // 添加对 args 的使用
        timeout: i32,
        run_as_user: bool,
        env_vars: Vec<String>,
        nushell_enable_config: bool,
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
        println!("path is {}", script_path.display());
        // 创建一个绑定来延长临时值的生命周期
        let path_string = script_path.to_string_lossy();
        let final_path = path_string.to_string().replace("C:", "C:\\");

        // 打印最终的路径以供调试
        println!("Final path for Deno script: {}", final_path);

        // 根据不同 shell 准备��行参数
        let (exe, cmd_args) = match shell {
            "powershell" => (
                "powershell.exe",
                vec![
                    "-NonInteractive".to_string(),
                    "-NoProfile".to_string(),
                    "-ExecutionPolicy".to_string(),
                    "Bypass".to_string(),
                    final_path.to_string(),
                ]
            ),
            "python" => (
                self.config.py_bin.as_str(),
                vec![final_path.to_string()]
            ),
            "cmd" => (
                final_path.as_ref(),
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
                nushell_args.push(final_path.to_string());
                (self.config.nu_bin.as_str(), nushell_args)
            },
            "deno" => {
                if !Path::new(&self.config.deno_bin).exists() {
                    return Err("Deno executable not found".to_string());
                }

                let mut deno_args = vec![
                    "run".to_string(),
                    "--no-prompt".to_string(),
                    "--allow-all".to_string(), // Keep only this one if you want all permissions
                    final_path.to_string(),
                ];
                // 添加额外的参数
                deno_args.extend(args);
                // 输出 deno 执行的完整命令，用于调试
                println!("Executing command: {} {:?}", self.config.deno_bin.as_str(), deno_args);
                (self.config.deno_bin.as_str(), deno_args)
            },


            _ => return Err(format!("不支持的脚本类型: {}", shell)),
        };

        // 打印执行的命令和参数，方便调试
        println!("Executing command: {} {:?}", exe, cmd_args);

        // 执行命令
        let output = self.execute_command(
            exe,
            cmd_args,
            timeout,
            env_vars
        )?;

        Ok(output)
    }
    // ���助方法：执行命令
    fn execute_command(
        &self,
        exe: &str,
        args: Vec<String>,
        timeout: i32,
        env_vars: Vec<String>
    ) -> Result<(String, String, i32), String> {
        use std::process::{Command, Stdio};
        use std::time::Duration;
        use std::io::Read;

        // 创建命令
        println!("Executing command: {} {:?}", exe, args);
        let mut cmd = Command::new(exe);
        cmd.args(&args);

        // 设置环境变量
        for var in env_vars {
            if let Some((key, value)) = var.split_once('=') {
                cmd.env(key, value);
            }
        }

        // 重定向输出
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        // 启动进程
        let mut child = cmd.spawn()
            .map_err(|e| format!("启动进程失败: {}", e))?;

        // 处理超时和输出
        let output = if timeout > 0 {
            // 使用 wait_timeout 处理超时
            match child.wait_timeout(Duration::from_secs(timeout as u64)) {
                Ok(Some(status)) => {
                    // 进程正常结束，获取输出
                    let mut stdout = String::new();
                    let mut stderr = String::new();

                    if let Some(mut stdout_pipe) = child.stdout.take() {
                        stdout_pipe.read_to_string(&mut stdout)
                            .map_err(|e| format!("读取标准输出失败: {}", e))?;
                    }

                    if let Some(mut stderr_pipe) = child.stderr.take() {
                        stderr_pipe.read_to_string(&mut stderr)
                            .map_err(|e| format!("读取错误输出失败: {}", e))?;
                    }

                    (stdout, stderr, status.code().unwrap_or(1))
                },
                Ok(None) => {
                    // 超时
                    child.kill().ok();
                    return Err("进程执行超时".to_string());
                },
                Err(e) => return Err(format!("等待程失败: {}", e)),
            }
        } else {
            // 无超时限制
            let output = child.wait_with_output()
                .map_err(|e| format!("获取进程输出失败: {}", e))?;

            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let exit_code = output.status.code().unwrap_or(1);

            (stdout, stderr, exit_code)
        };

        Ok(output)
    }
    // 辅助方法：下载并解压
    fn download_and_extract(
        &self,
        client: &reqwest::blocking::Client,
        url: &str,
        dest_dir: &Path,
        asset_name: &str,
        use_temp_dir: bool,
    ) -> Result<(), String> {
        let tmp_asset_path = if use_temp_dir {
            let tmp_dir = tempfile::Builder::new()
                .tempdir()
                .map_err(|e| format!("创建临时目录失败: {}", e))?;
            tmp_dir.path().join(asset_name)
        } else {
            dest_dir.join(asset_name)
        };

        let mut response = client.get(url)
            .send()
            .map_err(|e| format!("下载失败: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("下载失败，状态码: {}", response.status()));
        }

        let mut file = File::create(&tmp_asset_path)
            .map_err(|e| format!("创建文件失败: {}", e))?;
        response.copy_to(&mut file)
            .map_err(|e| format!("保存下载文件失败: {}", e))?;

        self.unzip(&tmp_asset_path, dest_dir.to_str().unwrap())?;
        Ok(())
    }
    
    // 添加新的辅助方法来检查脚本环境
    fn check_script_environment(&self, shell: &str) -> Result<(), String> {
        match shell {
            "python" => {
                if !Path::new(&self.config.py_bin).exists() {
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
                if !Path::new(&self.config.nu_bin).exists() {
                    return Err("Nushell 未安装，请先运行 install_nu_shell()".to_string());
                }
            },
            "deno" => {
                if !Path::new(&self.config.deno_bin).exists() {
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
    use std::fs;

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

    // 清理测试环境
    fn cleanup_test_environment() -> std::io::Result<()> {
        if let Some(install_dir) = get_install_dir_from_registry() {
            // 定义一个清理目录的闭包
            let cleanup_dir = |dir_path: &Path| -> std::io::Result<()> {
                if dir_path.exists() {
                    println!("清理目录: {:?}", dir_path);
                    fs::remove_dir_all(dir_path)
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
                } else {
                    Ok(())
                }
            };

            // 清理临时目录
            cleanup_dir(&install_dir.join("temp"))?;
            cleanup_dir(&install_dir.join("temp/user"))?;

            // 清理运行时目录
            let runtime_dirs = [
                "runtime/python",
                "runtime/nushell",
                "runtime/deno"
            ];

            for dir in runtime_dirs {
                cleanup_dir(&install_dir.join(dir))?;
            }
        }

        Ok(())
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

    fn create_test_case(shell: &'static str, script: &'static str, expected_output: &'static str, install_fn: fn(&ScriptExecutor) -> Result<(), String>) -> ScriptTest {
        ScriptTest {
            shell,
            script,
            expected_output,
            args: vec![],
            env_vars: vec![],
            install_fn,
        }
    }

    #[test]
    fn test_all_shells() {
        // 清理环境
        cleanup_test_environment().expect("清理临时目录失败");

        // Python 测试用例
        let python_test = create_test_case(
            "python",
            "print('Hello from Python! ')",
            "Hello from Python! ",
            |executor| {
                if !Path::new(&executor.config.py_bin).exists() {
                    executor.get_python(false).map_err(|e| format!("python安装失败：{}", e))?;
                }
                Ok(())
            },
        );

        // Nushell 测试用例
        let nushell_test = create_test_case(
            "nushell",
            "echo 'Hello from Nushell! '",
            "Hello from Nushell! ",
            |executor| {
                if !Path::new(&executor.config.nu_bin).exists() {
                    executor.install_nu_shell(false).map_err(|e| format!("Nushell安装失败：{}", e))?;
                }
                Ok(())
            },
        );

        // Deno 测试用例
        let deno_test = create_test_case(
            "deno",
            "console.log('Hello from Deno! ')",
            "Hello from Deno! ",
            |executor| {
                if !Path::new(&executor.config.deno_bin).exists() {
                    executor.install_deno(false).map_err(|e| format!("Deno安装失败：{}", e))?;
                }
                Ok(())
            },
        );

        // 执行所有测试
        for test in [python_test, nushell_test, deno_test] {
            test_script_execution(test.clone())
                .unwrap_or_else(|e| panic!("{} 测试失败: {}", test.shell, e));
        }

        // 清理环境（可以选择在这里再次清理）
        cleanup_test_environment().expect("清理临时目录失败");
    }
}





