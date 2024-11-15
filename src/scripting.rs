// scripting.rs
use std::fs::{self, File};
use std::path::Path;
use rand::Rng;
use reqwest::blocking::Client;
use zip::read::ZipArchive;

pub struct Agent {
    pub py_dir: String,
    pub py_bin: String,
    pub py_base_dir: String,
    pub proxy: Option<String>,
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
}

// 测试代码
#[cfg(test)]
mod tests {
    use super::*; // 引入外部模块

    #[test]
    fn test_agent_get_python() {
        // 创建一个测试用的 Agent 实例
        let agent = Agent {
            py_dir: "python3.11.9".to_string(),
            py_bin: "python3.11.9/bin/python".to_string(),
            py_base_dir: "C:\\Users\\29693\\Desktop\\ce_shi".to_string(),
            proxy: None,
        };

        // 调用 get_python 方法并测试其行为
        agent.get_python(false);  // `false` 表示不强制重新下载

        // 可以添加一些断言，例如检查某些文件是否被创建，或者某些状态是否改变
        // 这里假设我们期望 Python 文件被解压到指定目录
        assert!(Path::new(&agent.py_base_dir).exists(), "Base directory should exist");
    }

    #[test]
    fn test_file_exists() {
        // 测试 file_exists 方法
        let agent = Agent {
            py_dir: "python3.11.9".to_string(),
            py_bin: "python3.11.9/bin/python".to_string(),
            py_base_dir: "C:\\Users\\29693\\Desktop\\ce_shi".to_string(),
            proxy: None,
        };

        assert!(agent.file_exists(&agent.py_base_dir), "Path should exist");
    }
}
