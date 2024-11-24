// scripting.rs
use std::fs::{self, File};
use std::path::Path;
use rand::Rng;
use reqwest::blocking::Client;
use zip::read::ZipArchive;
use std::path::PathBuf;
use tempfile::Builder;
use std::io::Write;
use defer::defer;
use wait_timeout::ChildExt;
use std::io::Read;

#[derive(Default)]
pub struct ScriptExecutor {
    // 基本路径
    pub py_bin: String,
    pub nu_bin: String,
    pub deno_bin: String,
    pub program_dir: String,
    
    // 临时目录
    pub win_tmp_dir: String,
    pub win_run_as_user_tmp_dir: String,
    
    // 代理设置
    pub proxy: Option<String>,  // 新增：代理设置
}


impl ScriptExecutor {
    pub fn get_python(&self, force: bool) {
        if self.file_exists(&self.py_bin) && !force {
            return;
        }

        if force {
            if let Some(parent) = Path::new(&self.py_bin).parent() {
                fs::remove_dir_all(parent).expect("Failed to remove directory");
            }
        }

        let sleep_delay = rand::thread_rng().gen_range(1..=10);
        println!("GetPython() sleeping for {} seconds", sleep_delay);
        std::thread::sleep(std::time::Duration::new(sleep_delay as u64, 0));

        // 确保父目录存在
        if let Some(parent) = Path::new(&self.py_bin).parent() {
            fs::create_dir_all(parent).expect("Failed to create base directory");
        }

        let arch_zip = "py3.11.9_amd64.zip";
        let py_zip = Path::new(&self.py_bin).parent().unwrap().join(arch_zip);

        let py_zip_clone = py_zip.clone();
        let _cleanup = std::panic::catch_unwind(move || {
            fs::remove_file(&py_zip_clone).ok();
        });

        let client = Client::builder();

        // 配置代理
        let client = if let Some(proxy_url) = &self.proxy {
            println!("使用代理: {}", proxy_url);
            client.proxy(reqwest::Proxy::all(proxy_url)
                .expect("代理设置无效"))
        } else {
            client
        };

        let client = client.build().expect("创建 HTTP 客户端失败");

        let url = "https://github.com/amidaware/rmmagent/releases/download/v2.8.0/py3.11.9_amd64.zip";
        println!("Downloading from URL: {}", url);

        let mut response = client.get(url)
            .send()
            .expect("Unable to download py3.11.9_amd64.zip from GitHub");

        if !response.status().is_success() {
            println!("Unable to download py3.11.9_amd64.zip from GitHub. Status code: {}", response.status());
            return;
        }

        let mut file = File::create(&py_zip).expect("Failed to create zip file");
        response.copy_to(&mut file).expect("Failed to save zip file");

        // 解压前确保每个子目录都存在
        if let Err(err) = self.unzip(&py_zip, &self.py_bin) {
            println!("{}", err);
        }
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
                // 处目录项，创建目录
                fs::create_dir_all(&out_path).map_err(|e| format!("Failed to create dir: {}", e))?;
            } else {
                let mut output_file = File::create(&out_path)
                    .map_err(|e| format!("Failed to create file: {}", e))?;
                std::io::copy(&mut file, &mut output_file)
                    .map_err(|e| format!("Failed to extract file: {}", e))?;
            }
        }

        Ok(())
    }

    pub fn install_nu_shell(&self, force: bool) -> Result<(), String> {
        if self.file_exists(&self.nu_bin) && !force {
            return Ok(());
        }

        if force && self.file_exists(&self.nu_bin) {
            fs::remove_file(&self.nu_bin)
                .map_err(|e| format!("Error removing nu.exe binary: {}", e))?;
        }

        // 随机延迟
        let sleep_delay = rand::thread_rng().gen_range(1..=10);
        println!("InstallNuShell() sleeping for {} seconds", sleep_delay);
        std::thread::sleep(std::time::Duration::new(sleep_delay as u64, 0));

        // 创建程序目录
        let program_bin_dir = Path::new(&self.program_dir).join("bin");
        if !program_bin_dir.exists() {
            fs::create_dir_all(&program_bin_dir)
                .map_err(|e| format!("Error creating Program Files bin folder: {}", e))?;
        }

        // 创建配置目录和文件
        let nu_shell_path = Path::new(&self.program_dir).join("etc").join("nu_shell");
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
        let client = if let Some(proxy_url) = &self.proxy {
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

        // 复制可执行文件
        fs::copy(
            tmp_dir.path().join("nu.exe"),
            &self.nu_bin
        ).map_err(|e| format!("Failed to copy nu.exe: {}", e))?;

        Ok(())
    }

    pub fn install_deno(&self, force: bool) -> Result<(), String> {
        if self.file_exists(&self.deno_bin) && !force {
            return Ok(());
        }

        if force && self.file_exists(&self.deno_bin) {
            fs::remove_file(&self.deno_bin)
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

        let client = if let Some(proxy_url) = &self.proxy {
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
        if let Some(parent) = Path::new(&self.deno_bin).parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("创建目标目录失败: {}", e))?;
        }

        // 复制可执行文件
        fs::copy(
            tmp_dir.path().join("deno.exe"),
            &self.deno_bin
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
        // 首先检查脚本环境是否已安装
        self.check_script_environment(shell)?;

        // 1. 获取文件扩展名
        let extension = match shell {
            "powershell" => ".ps1",
            "python" => ".py",
            "cmd" => ".bat",
            "nushell" => ".nu",
            "deno" => ".ts",
            _ => return Err(format!("不支持的脚本类型: {}", shell)),
        };

        // 2. 根据 run_as_user 选择合适的临时目录
        let temp_dir = if run_as_user {
            PathBuf::from(&self.win_run_as_user_tmp_dir)
        } else {
            PathBuf::from(&self.win_tmp_dir)
        };

        if !temp_dir.exists() {
            fs::create_dir_all(&temp_dir)
                .map_err(|e| format!("创建临时目录失败: {}", e))?;
        }

        // 3. 创建临时文件
        let temp_file = Builder::new()
            .prefix("script_")
            .suffix(extension)
            .tempfile_in(&temp_dir)
            .map_err(|e| format!("创建临时文件失败: {}", e))?;

        // 4. 写入脚本内容
        temp_file.as_file()
            .write_all(code.as_bytes())
            .map_err(|e| format!("写入脚本内容失败: {}", e))?;

        // 5. 转为 PathBuf 并返回
        let script_path = temp_file.into_temp_path();

        // 确保临时文件会被清理
        let _cleanup = defer(|| {
            if let Err(e) = std::fs::remove_file(&script_path) {
                println!("清理临时文件失败: {}", e);
            }
        });

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
                self.py_bin.as_str(),
                vec![path_string.to_string()]
            ),
            "cmd" => (
                path_string.as_ref(),
                Vec::new()
            ),
            "nushell" => {
                if !Path::new(&self.nu_bin).exists() {
                    return Err("Nushell executable not found".to_string());
                }
                let mut nushell_args = if nushell_enable_config {
                    vec![
                        "--config".to_string(),
                        format!("{}/etc/nushell/config.nu", self.program_dir),
                        "--env-config".to_string(),
                        format!("{}/etc/nushell/env.nu", self.program_dir),
                    ]
                } else {
                    vec!["--no-config-file".to_string()]
                };
                nushell_args.push(path_string.to_string());
                (self.nu_bin.as_str(), nushell_args)
            },
            "deno" => {
                if !Path::new(&self.deno_bin).exists() {
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
                (self.deno_bin.as_str(), deno_args)
            },
            _ => return Err(format!("Unsupported shell type: {}", shell)),
        };

        // 添加额外参数
        cmd_args.extend(args);

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
                if !self.file_exists(&self.py_bin) {
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
                if !self.file_exists(&self.nu_bin) {
                    return Err("Nushell 未安装，请先运行 install_nu_shell()".to_string());
                }
            },
            "deno" => {
                if !self.file_exists(&self.deno_bin) {
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

    fn create_test_executor() -> ScriptExecutor {
        ScriptExecutor {
            py_bin: "C:\\Users\\29693\\Desktop\\test_dir\\py3.11.9_amd64\\python.exe".to_string(),
            nu_bin: "C:\\Users\\29693\\Desktop\\test_nushell\\bin\\nu.exe".to_string(),
            deno_bin: "C:\\Users\\29693\\Desktop\\test_deno\\bin\\deno.exe".to_string(),
            program_dir: "C:\\Users\\29693\\Desktop\\test_nushell".to_string(),
            win_tmp_dir: String::from(".\\temp"),
            win_run_as_user_tmp_dir: String::from(".\\temp\\user"),
            proxy: None,
        }
    }

    #[test]
    fn test_python_installation_and_script() {
        let executor = create_test_executor();
        
        println!("=== 测试 Python 安装 ===");
        
        // 首先确保临时目录存在
        fs::create_dir_all(&executor.win_tmp_dir)
            .expect("无法创建临时目录");
        
        executor.get_python(false);
        
        // 创建并写入 Python 测试脚本
        let python_script_path = Path::new(&executor.win_tmp_dir).join("test_script.py");
        println!("\n创建 Python 测试脚本: {}", python_script_path.display());
        
        let python_script_content = r#"
print("Hello from Python!")
print("Python installation test successful!")
"#;
        
        match fs::write(&python_script_path, python_script_content) {
            Ok(_) => println!("Python 脚本创建成功"),
            Err(e) => println!("Python 脚本创建失败: {}", e),
        }
        
        // 验证文件是否存在和内容
        assert!(python_script_path.exists(), "Python 脚本文件不存在");
        if let Ok(content) = fs::read_to_string(&python_script_path) {
            println!("\nPython 脚本内容:\n{}", content);
        }

        // 测试完成后清理临时文件
        let _ = fs::remove_file(&python_script_path);
    }

    #[test]
    fn test_nushell_installation_and_script() {
        let executor = create_test_executor();
        
        println!("=== 测试 Nu Shell 安装 ===");
        match executor.install_nu_shell(false) {
            Ok(_) => {
                // 创建脚本目录
                let scripts_dir = Path::new(&executor.win_tmp_dir).join("scripts");
                fs::create_dir_all(&scripts_dir).expect("无法创建脚本目录");
                
                // 创建并写入 Nu Shell 测试脚本
                let nu_script_path = scripts_dir.join("test_script.nu");
                println!("\n创建 Nu Shell 测试脚本: {}", nu_script_path.display());
                
                let nu_script_content = r#"
echo "Hello from Nu Shell!"
echo "Nu Shell installation test successful!"
"#;
                
                match fs::write(&nu_script_path, nu_script_content) {
                    Ok(_) => println!("Nu Shell 脚本创建成功"),
                    Err(e) => println!("Nu Shell 脚本创建失败: {}", e),
                }
                
                // 验证文件是否存在和内容
                assert!(nu_script_path.exists(), "Nu Shell 脚本文件不存在");
                if let Ok(content) = fs::read_to_string(&nu_script_path) {
                    println!("\nNu Shell 脚本内容:\n{}", content);
                }
            }
            Err(e) => panic!("Nu Shell 安装失败: {}", e),
        }
    }

    #[test]
    fn test_deno_installation_and_script() {
        let executor = create_test_executor();
        
        println!("=== 测试 Deno 安装 ===");
        match executor.install_deno(false) {
            Ok(_) => {
                // 创建脚本目录
                let scripts_dir = Path::new(&executor.win_tmp_dir).join("scripts");
                fs::create_dir_all(&scripts_dir).expect("无法创建脚本目录");
                
                // 创建并写入 Deno 测试脚本
                let deno_script_path = scripts_dir.join("test_script.ts");
                println!("\n创建 Deno 测试脚本: {}", deno_script_path.display());
                
                let deno_script_content = r#"
console.log("Hello from Deno!");
console.log("Deno installation test successful!");
"#;
                
                match fs::write(&deno_script_path, deno_script_content) {
                    Ok(_) => println!("Deno 脚本创建成功"),
                    Err(e) => println!("Deno 脚本创建失败: {}", e),
                }
                
                // 验证文件是否存在和内容
                assert!(deno_script_path.exists(), "Deno 脚本文件不存在");
                if let Ok(content) = fs::read_to_string(&deno_script_path) {
                    println!("\nDeno 脚本内容:\n{}", content);
                }
            }
            Err(e) => panic!("Deno 安装失败: {}", e),
        }
    }

    // 建议添加新的测试用例来测试 run_as_user 功能
    #[test]
    fn test_run_script_as_user() {
        let executor = create_test_executor();
        
        // 测试普通用户和提升权限用户的临时目录
        let script_content = "print('Hello, World!')";
        
        // 测试普通用户执行
        let (stdout, _stderr, exit_code) = executor.run_script(
            script_content,
            "python",
            vec![],
            30,
            false,  // run_as_user = false
            vec![],
            false,
            "",
        ).expect("脚本执行失败");
        
        assert_eq!(exit_code, 0, "普通用户脚本执行失败");
        assert!(stdout.contains("Hello, World!"));
        
        // 测试提升权限用户执行
        let (stdout, _stderr, exit_code) = executor.run_script(
            script_content,
            "python",
            vec![],
            30,
            true,  // run_as_user = true
            vec![],
            false,
            "",
        ).expect("提升权限脚本执行失败");
        
        assert_eq!(exit_code, 0, "提升权限脚本执行失败");
        assert!(stdout.contains("Hello, World!"));
    }

    #[test]
    fn test_run_script_with_different_shells() {
        let executor = create_test_executor();

        // 1. 测试 Python 脚本执行
        println!("=== 测试 Python 脚本执行 ===");
        let python_test = executor.run_script(
            "print('Hello from Python!')",
            "python",
            vec!["-u".to_string()],  // 无缓冲输出
            30,
            false,
            vec!["PYTHON_TEST=true".to_string()],
            false,
            "",
        );
        match python_test {
            Ok((stdout, stderr, exit_code)) => {
                println!("Python 输出: {}", stdout);
                println!("Python 错误: {}", stderr);
                println!("退出码: {}", exit_code);
                assert!(stdout.contains("Hello from Python!"));
                assert_eq!(exit_code, 0);
            },
            Err(e) => panic!("Python 脚本执行失败: {}", e),
        }

        // 2. 测试 Deno 脚本执行
        println!("\n=== 测试 Deno 脚本执行 ===");
        let deno_test = executor.run_script(
            "console.log('Hello from Deno!')",
            "deno",
            vec!["--allow-net".to_string()],
            30,
            false,
            vec!["DENO_TEST=true".to_string()],
            false,
            "--allow-net --allow-read",
        );
        match deno_test {
            Ok((stdout, stderr, exit_code)) => {
                println!("Deno 输出: {}", stdout);
                println!("Deno 错误: {}", stderr);
                println!("退出码: {}", exit_code);
                assert!(stdout.contains("Hello from Deno!"));
                assert_eq!(exit_code, 0);
            },
            Err(e) => panic!("Deno 脚本执行失败: {}", e),
        }

        // 3. 测试 Nushell 脚本执行
        println!("\n=== 测试 Nushell 脚本执行 ===");
        let nu_test = executor.run_script(
            "echo 'Hello from Nushell!'",
            "nushell",
            vec![],
            30,
            false,
            vec!["NU_TEST=true".to_string()],
            true,
            "",
        );
        match nu_test {
            Ok((stdout, stderr, exit_code)) => {
                println!("Nushell 输出: {}", stdout);
                println!("Nushell 错误: {}", stderr);
                println!("退出码: {}", exit_code);
                assert!(stdout.contains("Hello from Nushell!"));
                assert_eq!(exit_code, 0);
            },
            Err(e) => panic!("Nushell 脚本执行失败: {}", e),
        }
    }

    #[test]
    fn test_run_script_with_timeout() {
        let executor = create_test_executor();
        
        println!("=== 测试脚本超时功能 ===");
        let timeout_test = executor.run_script(
            "import time\ntime.sleep(5)\nprint('This should not be printed')",
            "python",
            vec![],
            2,  // 2秒超时
            false,
            vec![],
            false,
            "",
        );

        match timeout_test {
            Ok((stdout, stderr, exit_code)) => {
                println!("超时测试输出: {}", stdout);
                println!("超时测试错误: {}", stderr);
                println!("退出码: {}", exit_code);
                assert!(stderr.contains("timed out"));
                assert_eq!(exit_code, 98);  // 超时退出码
            },
            Err(e) => panic!("超时测试执行失败: {}", e),
        }
    }

    #[test]
    fn test_run_script_with_env_vars() {
        let executor = create_test_executor();
        
        println!("=== 测试环境变量传递 ===");
        let env_test = executor.run_script(
            "import os\nprint(f\"TEST_VAR = {os.getenv('TEST_VAR')}\")",
            "python",
            vec![],
            30,
            false,
            vec!["TEST_VAR=hello_world".to_string()],
            false,
            "",
        );

        match env_test {
            Ok((stdout, stderr, exit_code)) => {
                println!("环境变量测试输出: {}", stdout);
                println!("环境变量测试错误: {}", stderr);
                println!("退出码: {}", exit_code);
                assert!(stdout.contains("TEST_VAR = hello_world"));
                assert_eq!(exit_code, 0);
            },
            Err(e) => panic!("环境变量测试执行失败: {}", e),
        }
    }

    #[test]
    fn test_run_script_with_invalid_shell() {
        let executor = create_test_executor();
        
        println!("=== 测试无效脚本类型 ===");
        let invalid_shell_test = executor.run_script(
            "echo 'test'",
            "invalid_shell",
            vec![],
            30,
            false,
            vec![],
            false,
            "",
        );

        assert!(invalid_shell_test.is_err());
        if let Err(e) = invalid_shell_test {
            assert!(e.contains("不支持的脚本类型"));
        }
    }
}

