// scripting.rs
use std::fs::{self, File};
use std::path::Path;
use rand::Rng;
use reqwest::blocking::Client;
use zip::read::ZipArchive;
use std::path::PathBuf;
use tempfile::Builder;
use std::io::Write;

pub struct Agent {
    pub py_dir: String,
    pub py_bin: String,
    pub py_base_dir: String,
    pub proxy: Option<String>,
    pub program_dir: String,
    pub nu_shell_bin: String,
    pub deno_bin: String,
    pub win_tmp_dir: String,
}

impl Agent {
    pub fn get_python(&self, force: bool) {
        if self.file_exists(&self.py_bin) && !force {
            return;
        }

        if force {
            fs::remove_dir_all(&self.py_base_dir).expect("Failed to remove directory");
        }

        let sleep_delay = rand::thread_rng().gen_range(1..=10);
        println!("GetPython() sleeping for {} seconds", sleep_delay);
        std::thread::sleep(std::time::Duration::new(sleep_delay as u64, 0));

        // 确保 py_base_dir 存在，若不存在则创建
        if !Path::new(&self.py_base_dir).exists() {
            fs::create_dir_all(&self.py_base_dir).expect("Failed to create base directory");
        }

        let arch_zip = "py3.11.9_amd64.zip"; // 修改为新的文件名
        let py_zip = Path::new(&self.py_base_dir).join(arch_zip);

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
        if let Err(err) = self.unzip(&py_zip, &self.py_base_dir) {
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
                // 处理目录项，创建目录
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
        // 随机延迟
        let sleep_delay = rand::thread_rng().gen_range(1..=10);
        println!("InstallNuShell() sleeping for {} seconds", sleep_delay);
        std::thread::sleep(std::time::Duration::new(sleep_delay as u64, 0));

        // 检查是否已安装
        if self.file_exists(&self.nu_shell_bin) {
            if force {
                println!("Forced install. Removing nu.exe binary.");
                fs::remove_file(&self.nu_shell_bin)
                    .map_err(|e| format!("Error removing nu.exe binary: {}", e))?;
            } else {
                return Ok(());
            }
        }

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
            &self.nu_shell_bin
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

    fn create_temp_script_file(&self, code: &str, shell: &str) -> Result<PathBuf, String> {
        // 1. 获取文件扩展名
        let extension = match shell {
            "powershell" => ".ps1",
            "python" => ".py",
            "cmd" => ".bat",
            "nushell" => ".nu",
            "deno" => ".ts",
            _ => return Err(format!("不支持的脚本类型: {}", shell))
        };

        // 2. 确保临时目录存在
        let temp_dir = PathBuf::from(&self.win_tmp_dir);
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

        // 5. 转换为 PathBuf 并返回
        let path = temp_file.into_temp_path();
        Ok(path.to_path_buf())
    }
}

// 测试代码
#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_agent() -> Agent {
        Agent {
            py_dir: "py3.11.9_amd64".to_string(),
            py_bin: "py3.11.9_amd64/bin/python".to_string(),
            py_base_dir: "C:\\Users\\29693\\Desktop\\test_dir".to_string(),
            proxy: None,
            program_dir: "C:\\Users\\29693\\Desktop\\test_nushell".to_string(),
            nu_shell_bin: "C:\\Users\\29693\\Desktop\\test_nushell\\bin\\nu.exe".to_string(),
            deno_bin: "C:\\Users\\29693\\Desktop\\test_deno\\bin\\deno.exe".to_string(),
            win_tmp_dir: String::from("./temp"),
        }
    }

    #[test]
    fn test_python_installation_and_script() {
        let agent = create_test_agent();
        
        println!("=== 测试 Python 安装 ===");
        agent.get_python(false);
        
        // 创建并写入 Python 测试脚本
        let python_script_path = Path::new(&agent.py_base_dir).join("test_script.py");
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
    }

    #[test]
    fn test_nushell_installation_and_script() {
        let agent = create_test_agent();
        
        println!("=== 测试 Nu Shell 安装 ===");
        match agent.install_nu_shell(false) {
            Ok(_) => {
                // 创建脚本目录
                let scripts_dir = Path::new(&agent.program_dir).join("scripts");
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
        let agent = create_test_agent();
        
        println!("=== 测试 Deno 安装 ===");
        match agent.install_deno(false) {
            Ok(_) => {
                // 使用新的目标目录创建脚本目录
                let deno_base_dir = "C:\\Users\\29693\\Desktop\\test_deno";
                let scripts_dir = Path::new(deno_base_dir).join("scripts");
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
                
                // 打印最终的目录结构
                println!("\n=== Deno 安装目录结构 ===");
                println!("可执行文件: {}", agent.deno_bin);
                println!("脚本文件: {}", deno_script_path.display());
            }
            Err(e) => panic!("Deno 安装失败: {}", e),
        }
    }

    #[test]
    fn test_create_temp_script_file() {
        // 创建测试用的 Agent 实例
        let agent = Agent {
            win_tmp_dir: String::from("./temp"),  // 测试用临时目录
            ..Default::default()
        };

        // 测试脚本内容
        let test_code = "print('Hello, World!')";
        
        // 测试 Python 脚本创建
        let result = agent.create_temp_script_file(test_code, "python");
        assert!(result.is_ok());
        
        let path = result.unwrap();
        assert!(path.exists());
        assert!(path.extension().unwrap() == "py");
        
        // 验证文件内容
        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(content, test_code);
        
        // 清理测试文件
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn test_invalid_shell_type() {
        let agent = Agent {
            win_tmp_dir: String::from("./temp"),
            ..Default::default()
        };

        let result = agent.create_temp_script_file("test", "invalid_shell");
        assert!(result.is_err());
    }
}
