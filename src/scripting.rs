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

    pub fn install_python(&self, force: bool) -> Result<(), String> {
        // If it already exists and is not forced to download, return success
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

        // Get runtime directory, here we refer to the parent directory of py_bin path, but we need to ensure that only "runtime" is obtained instead of "runtime\\python"
        let py_bin_path = Path::new(&self.config.py_bin);
        let runtime_dir = py_bin_path.parent()
            .and_then(|parent| parent.parent())  // Remove "python" directory, get "runtime" directory
            .ok_or_else(|| "Failed to get runtime directory".to_string())?;

        // Before downloading and extracting, ensure the target directory is clean
        let python_dir = runtime_dir.join("python");
        let extracted_dir = runtime_dir.join("py3.11.9_amd64");

        // If force install, clean existing directories first
        if force {
            println!("Force install: cleaning existing directories");
            if python_dir.exists() {
                fs::remove_dir_all(&python_dir)
                    .map_err(|e| format!("Failed to remove existing python directory: {}", e))?;
            }
            if extracted_dir.exists() {
                fs::remove_dir_all(&extracted_dir)
                    .map_err(|e| format!("Failed to remove existing extracted directory: {}", e))?;
            }
        }

        // Create runtime directory
        println!("Creating runtime directory: {}", runtime_dir.display());
        fs::create_dir_all(&runtime_dir)
            .map_err(|e| format!("Failed to create runtime directory: {}", e))?;

        // Download and extract
        let client = self.create_http_client()?;
        let url = "https://github.com/amidaware/rmmagent/releases/download/v2.8.0/py3.11.9_amd64.zip";
        
        println!("Downloading Python from: {}", url);
        self.download_and_extract(&client, url, &runtime_dir, "py3.11.9_amd64.zip", false)?;

        // Check target directory again before renaming
        if python_dir.exists() {
            println!("Removing existing Python directory before rename");
            fs::remove_dir_all(&python_dir)
                .map_err(|e| format!("Failed to remove existing python directory: {}", e))?;
        }

        // Ensure source directory exists
        if !extracted_dir.exists() {
            return Err("Extracted directory not found".to_string());
        }

        // Rename directory
        println!("Renaming {} to {}", extracted_dir.display(), python_dir.display());
        if let Err(e) = fs::rename(&extracted_dir, &python_dir) {
            return Err(format!("Failed to rename directory: {} (Error: {})", extracted_dir.display(), e));
        }

        println!("Python installation completed successfully");
        Ok(())
    }
    pub fn install_nu_shell(&self, force: bool) -> Result<(), String> {
        // If it already exists and is not forced to download, return success
        if Path::new(&self.config.nu_bin).exists() && !force {
            return Ok(());
        }

        if force {
            if let Some(parent) = Path::new(&self.config.nu_bin).parent() {
                fs::remove_dir_all(parent).map_err(|e| format!("Failed to remove directory: {}", e))?;
            }
        }

        let sleep_delay = rand::thread_rng().gen_range(1..=10);
        println!("InstallNuShell() sleeping for {} seconds", sleep_delay);
        std::thread::sleep(std::time::Duration::new(sleep_delay as u64, 0));

        // Get runtime directory and create nushell directory
        let nu_bin_path = Path::new(&self.config.nu_bin);
        let runtime_dir = nu_bin_path.parent()
            .and_then(|parent| parent.parent())  // Remove "nushell" directory, get "runtime" directory
            .ok_or_else(|| "Failed to get runtime directory".to_string())?;
        
        let nushell_dir = runtime_dir.join("nushell");
        
        // Create nushell directory
        fs::create_dir_all(&nushell_dir).map_err(|e| format!("Failed to create nushell directory: {}", e))?;

        // Create configuration directory and files
        let nu_shell_path = Path::new(&self.config.program_dir).join("etc").join("nushell");
        let nu_shell_config = nu_shell_path.join("config.nu");
        let nu_shell_env = nu_shell_path.join("env.nu");

        // Create configuration directory
        if !nu_shell_path.exists() {
            fs::create_dir_all(&nu_shell_path)
                .map_err(|e| format!("Error creating nu_shell config directory: {}", e))?;
        }

        // Create configuration files if they don't exist
        for config_file in &[nu_shell_config, nu_shell_env] {
            if !config_file.exists() {
                File::create(config_file)
                    .map_err(|e| format!("Error creating config file: {}", e))?;
            }
        }

        // Build download URL and asset name
        let version = "0.87.0";
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

        // Create HTTP client
        let client = self.create_http_client()?;

        // Download and extract directly to nushell directory
        self.download_and_extract(&client, &url, &nushell_dir, &asset_name, false)?;

        println!("nu.exe successfully installed to target path: {:?}", self.config.nu_bin);
        Ok(())
    }

    pub fn install_deno(&self, force: bool) -> Result<(), String> {
        // If it already exists and is not forced to download, return success
        if Path::new(&self.config.deno_bin).exists() && !force {
            return Ok(());
        }

        if force {
            if let Some(parent) = Path::new(&self.config.deno_bin).parent() {
                fs::remove_dir_all(parent).map_err(|e| format!("Failed to remove directory: {}", e))?;
            }
        }

        let sleep_delay = rand::thread_rng().gen_range(1..=10);
        println!("InstallDeno() sleeping for {} seconds", sleep_delay);
        std::thread::sleep(std::time::Duration::new(sleep_delay as u64, 0));

        // Get runtime directory and create deno directory
        let deno_bin_path = Path::new(&self.config.deno_bin);
        let runtime_dir = deno_bin_path.parent()
            .and_then(|parent| parent.parent())  // Remove "deno" directory, get "runtime" directory
            .ok_or_else(|| "Failed to get runtime directory".to_string())?;
        
        let deno_dir = runtime_dir.join("deno");
        
        // Create deno directory
        fs::create_dir_all(&deno_dir).map_err(|e| format!("Failed to create deno directory: {}", e))?;

        // Deno download url
        let url = "https://github.com/denoland/deno/releases/download/v2.1.3/deno-x86_64-pc-windows-msvc.zip";
        println!("Deno download url: {}", url);

        // Create HTTP client
        let client = self.create_http_client()?;

        // Download and extract directly to deno directory
        self.download_and_extract(&client, url, &deno_dir, "deno-x86_64-pc-windows-msvc.zip", false)?;

        println!("deno.exe successfully installed to target path: {:?}", self.config.deno_bin);
        Ok(())
    }
    // Auxiliary method: unzip
    fn unzip(&self, zip_path: &Path, dest_dir: &str) -> Result<(), String> {
        let file = File::open(zip_path).map_err(|e| format!("Failed to open zip file: {}", e))?;
        let mut archive = ZipArchive::new(file).map_err(|e| format!("Failed to read zip archive: {}", e))?;

        for i in 0..archive.len() {
            let mut file = archive.by_index(i).map_err(|e| format!("Failed to read file from zip: {}", e))?;
            let out_path = Path::new(dest_dir).join(file.name());

            // Ensure that each subdirectory exists when decompressing
            if let Some(parent_dir) = out_path.parent() {
                if !parent_dir.exists() {
                    fs::create_dir_all(parent_dir).map_err(|e| format!("Failed to create directory: {}", e))?;
                }
            }

            if file.name().ends_with('/') {
                // It is a directory item, create directory
                fs::create_dir_all(&out_path).map_err(|e| format!("Failed to create dir: {}", e))?;
            } else {
                // It is a file item, write to file
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
        args: Vec<String>,  // Add usage of args
        timeout: i32,
        run_as_user: bool,
        env_vars: Vec<String>,
        nushell_enable_config: bool,
    ) -> Result<(String, String, i32), String> {
        // First check script environment
        self.check_script_environment(shell)?;

        let tmp_dir = if run_as_user {
            &self.config.win_run_as_user_tmp_dir
        } else {
            &self.config.win_tmp_dir
        };

        // Ensure temporary directory exists
        if !Path::new(tmp_dir).exists() {
            fs::create_dir_all(tmp_dir)
                .map_err(|e| format!("Failed to create temporary directory: {}", e))?;
        }

        // 1. Get file extension
        let extension = match shell {
            "powershell" => ".ps1",
            "python" => ".py",
            "cmd" => ".bat",
            "nushell" => ".nu",
            "deno" => ".ts",
            _ => return Err(format!("Unsupported script type: {}", shell)),
        };

        // 2. Create temporary file
        let temp_file = Builder::new()
            .prefix("script_")
            .suffix(extension)
            .tempfile_in(tmp_dir)
            .map_err(|e| format!("Failed to create temporary file: {}", e))?;

        // 3. Write script content
        let bom = vec![0xEF, 0xBB, 0xBF];//Add UTF-8 BOM mark
        temp_file.as_file()
            .write_all(&bom)
            .map_err(|e| format!("Failed to write BOM: {}", e))?;
        let script_content = if shell == "nushell" {
            format!("#!nushell\n{}", code)
        } else {
            code.to_string()
        };
        temp_file.as_file()
            .write_all(script_content.as_bytes())
            .map_err(|e| format!("Failed to write script content: {}", e))?;
        // 4. Convert to PathBuf and return
        let script_path = temp_file.into_temp_path();
        println!("path is {}", script_path.display());
        // Create a binding to extend the temporary value's lifetime
        let path_string = script_path.to_string_lossy();
        let final_path = if path_string.contains(' ') {
            format!("\"{}\"", path_string.replace("C:", "C:\\"))
        } else {
            path_string.replace("C:", "C:\\")
        };

        // Print the final path for debugging
        println!("Final path for Deno script: {}", final_path);

        // Prepare execution parameters for different shells
        let (exe, cmd_args) = match shell {
            "python" => {
                // For Python, use the path directly without additional quotes
                let script_path = path_string.replace("C:", "C:\\");
                (
                    self.config.py_bin.as_str(),
                    vec![script_path]
                )
            },
            "powershell" => {
                // Simplify path handling
                let script_path = path_string.replace("C:", "C:\\");
                (
                    "powershell.exe",
                    vec![
                        "-NonInteractive".to_string(),
                        "-NoProfile".to_string(),
                        "-ExecutionPolicy".to_string(),
                        "Bypass".to_string(),
                        "-File".to_string(),
                        script_path,
                    ]
                )
            },
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
                    vec![]
                };
                
                nushell_args.extend(vec![
                    "-c".to_string(),
                    code.to_string(),
                ]);
                
                (self.config.nu_bin.as_str(), nushell_args)
            },
            "deno" => {
                if !Path::new(&self.config.deno_bin).exists() {
                    return Err("Deno executable not found".to_string());
                }
                let script_path = path_string.replace("C:", "C:\\");
                let mut deno_args = vec![
                    "run".to_string(),
                    "--no-prompt".to_string(),
                    "--allow-all".to_string(),
                    script_path,
                ];
                deno_args.extend(args);
                // Output the complete command executed by deno, for debugging
                println!("Executing command: {} {:?}", self.config.deno_bin.as_str(), deno_args);
                (self.config.deno_bin.as_str(), deno_args)
            },
            "cmd" => (
                final_path.as_ref(),
                Vec::new()
            ),
            _ => return Err(format!("Unsupported script type: {}", shell)),
        };

        // Print the executed command and parameters, for debugging
        println!("Executing command: {} {:?}", exe, cmd_args);

        // Execute command
        let output = self.execute_command(
            exe,
            cmd_args,
            timeout,
            env_vars
        )?;

        Ok(output)
    }
    // Auxiliary method: execute command
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

        // Create command
        println!("Executing command: {} {:?}", exe, args);
        let mut cmd = Command::new(exe);
        cmd.args(&args);

        // Set environment variables
        for var in env_vars {
            if let Some((key, value)) = var.split_once('=') {
                cmd.env(key, value);
            }
        }

        // Redirect output
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        // Start process
        let mut child = cmd.spawn()
            .map_err(|e| format!("Failed to start process: {}", e))?;

        // Handle timeout and output
        let output = if timeout > 0 {
            match child.wait_timeout(Duration::from_secs(timeout as u64)) {
                Ok(Some(status)) => {
                    let mut stdout = Vec::new();
                    let mut stderr = Vec::new();

                    if let Some(mut stdout_pipe) = child.stdout.take() {
                        stdout_pipe.read_to_end(&mut stdout)
                            .map_err(|e| format!("Failed to read standard output: {}", e))?;
                    }

                    if let Some(mut stderr_pipe) = child.stderr.take() {
                        stderr_pipe.read_to_end(&mut stderr)
                            .map_err(|e| format!("Failed to read error output: {}", e))?;
                    }

                    let stdout_str = String::from_utf8_lossy(&stdout).into_owned();
                    let stderr_str = String::from_utf8_lossy(&stderr).into_owned();

                    (stdout_str, stderr_str, status.code().unwrap_or(1))
                },
                Ok(None) => {
                    child.kill().ok();
                    return Err("Process execution timeout".to_string());
                },
                Err(e) => return Err(format!("Failed to wait for process: {}", e)),
            }
        } else {
            let output = child.wait_with_output()
                .map_err(|e| format!("Failed to get process output: {}", e))?;

            let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            let exit_code = output.status.code().unwrap_or(1);

            (stdout, stderr, exit_code)
        };

        Ok(output)
    }
    // Auxiliary method: download and extract
    fn download_and_extract(
        &self,
        client: &reqwest::blocking::Client,
        url: &str,
        dest_dir: &Path,
        asset_name: &str,
        _use_temp_dir: bool,
    ) -> Result<(), String> {
        let asset_path = dest_dir.join(asset_name);
        println!("Downloading to: {}", asset_path.display());

        // Download file
        let mut response = client.get(url)
            .send()
            .map_err(|e| format!("Download failed: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("Download failed with status: {}", response.status()));
        }

        // Save file
        let mut file = File::create(&asset_path)
            .map_err(|e| format!("Failed to create file: {}", e))?;
        
        response.copy_to(&mut file)
            .map_err(|e| format!("Failed to save file: {}", e))?;

        // Extract file
        println!("Extracting file: {}", asset_path.display());
        self.unzip(&asset_path, dest_dir.to_str().unwrap())?;

        // Remove zip file
        println!("Removing zip file");
        if let Err(e) = fs::remove_file(&asset_path) {
            println!("Warning: Failed to remove zip file: {}", e);
            // Do not interrupt the entire process due to clean_up failure
        }

        Ok(())
    }
    
    // Add a new auxiliary method to check the script environment
    fn check_script_environment(&self, shell: &str) -> Result<(), String> {
        println!("Checking and installing necessary script environments...");
        
        match shell {
            "python" => {
                if !Path::new(&self.config.py_bin).exists() {
                    return Err("Python is not installed, please run install_python()".to_string());
                }
            },
            "powershell" => {
                // Windows system usually pre-installs PowerShell, but it can still be checked
                if !Path::new("powershell.exe").exists() && 
                   !Path::new("C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe").exists() {
                    return Err("PowerShell not found".to_string());
                }
            },
            "cmd" => {
                // Windows system usually pre-installs CMD, but it can still be checked
                if !Path::new("cmd.exe").exists() && 
                   !Path::new("C:\\Windows\\System32\\cmd.exe").exists() {
                    return Err("CMD not found".to_string());
                }
            },
            "nushell" => {
                if !Path::new(&self.config.nu_bin).exists() {
                    return Err("Nushell is not installed, please run install_nu_shell()".to_string());
                }
            },
            "deno" => {
                if !Path::new(&self.config.deno_bin).exists() {
                    return Err("Deno is not installed, please run install_deno()".to_string());
                }
            },
            _ => return Err(format!("Unsupported script type: {}", shell)),
        }
        Ok(())
    }

    fn create_http_client(&self) -> Result<Client, String> {
        let client_builder = Client::builder();
        let client = if let Some(proxy_url) = &self.config.proxy {
            client_builder
                .proxy(reqwest::Proxy::all(proxy_url)
                    .map_err(|e| format!("Proxy configuration failed: {}", e))?)
                .build()
                .map_err(|e| format!("Failed to create HTTP client: {}", e))?
        } else {
            client_builder
                .build()
                .map_err(|e| format!("Failed to create HTTP client: {}", e))?
        };
        Ok(client)
    }

    pub fn install_choco(&self) -> Result<bool, String> {
        println!("Starting Chocolatey installation...");
        
        // Create HTTP  
        let client = self.create_http_client()?;

        // Download Chocolatey installation script
        println!("Downloading Chocolatey installation script...");
        let response = client.get("https://chocolatey.org/install.ps1")
            .send()
            .map_err(|e| format!("Failed to download Chocolatey script: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("Failed to download Chocolatey script, status: {}", response.status()));
        }

        let script = response.text()
            .map_err(|e| format!("Failed to read response text: {}", e))?;

        // Execute installation script
        println!("Executing Chocolatey installation script...");
        let (stdout, stderr, exit_code) = self.run_script(
            &script,
            "powershell",
            vec![],
            900,  //15 minutes timeout
            false,
            vec![],
            false
        )?;

        if exit_code != 0 {
            println!("Installation stdout: {}", stdout);
            println!("Installation stderr: {}", stderr);
            return Err(format!("Chocolatey installation failed with exit code: {}", exit_code));
        }

        println!("Chocolatey installation completed successfully");
        Ok(true)
    }
}

// Test code
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // Auxiliary function to ensure environment installation
    fn ensure_environments(executor: &ScriptExecutor) -> Result<(), String> {
        println!("Checking and installing necessary script environments...");

        // Check and install Python
        if !Path::new(&executor.config.py_bin).exists() {
            println!("Installing Python...");
            executor.install_python(false).map_err(|e| format!("python installation failed: {}", e))?;
        }

        // Check and install Nushell
        if !Path::new(&executor.config.nu_bin).exists() {
            println!("Installing Nushell...");
            executor.install_nu_shell(false).map_err(|e| format!("Nushell installation failed: {}", e))?;
        }

        // Check and install Deno
        if !Path::new(&executor.config.deno_bin).exists() {
            println!("Installing Deno...");
            executor.install_deno(false).map_err(|e| format!("Deno installation failed: {}", e))?;
        }

        println!("All environment checks completed");
        Ok(())
    }

    fn create_test_executor() -> ScriptExecutor {
        let config = AgentConfig::default();
        let executor = ScriptExecutor::new(config);
    
        // Ensure environments are installed
        ensure_environments(&executor)
            .expect("Failed to setup script environments");

        executor
    }

    // Clean up test environment
    fn cleanup_test_environment() -> std::io::Result<()> {
        if let Some(install_dir) = get_install_dir_from_registry() {
            // Define a closure to clean up directories
            let cleanup_dir = |dir_path: &Path| -> std::io::Result<()> {
                if dir_path.exists() {
                    println!("Cleaning up directory: {:?}", dir_path);
                    fs::remove_dir_all(dir_path)
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
                } else {
                    Ok(())
                }
            };

            // Clean up temporary directories
            cleanup_dir(&install_dir.join("temp"))?;
            cleanup_dir(&install_dir.join("temp/user"))?;

            // Clean up runtime directories
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

    // Add Clone and Debug traits
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

        // 1. Install/prepare environment
        println!("=== Testing {} environment installation ===", test_case.shell);
        (test_case.install_fn)(&executor)?;

        // 2. Execute script test
        println!("=== Testing {} script execution ===", test_case.shell);
        let (stdout, stderr, exit_code) = executor.run_script(
            test_case.script,
            test_case.shell,
            test_case.args,
            30,
            false,
            test_case.env_vars,
            false,
        )?;
        // 3. Verify results
        println!("{} output: {}", test_case.shell, stdout);
        println!("{} error: {}", test_case.shell, stderr);
        println!("Exit code: {}", exit_code);

        assert!(stdout.contains(test_case.expected_output),
                "Expected output not found: {}", test_case.expected_output);
        assert_eq!(exit_code, 0, "Script execution failed");
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
        // Clean up environment
        cleanup_test_environment().expect("Failed to clean up temporary directories");

        // Python test case
        let python_test = create_test_case(
            "python",
            "print('Hello from Python! ')",
            "Hello from Python! ",
            |executor| {
                if !Path::new(&executor.config.py_bin).exists() {
                    executor.install_python(false).map_err(|e| format!("python installation failed: {}", e))?;
                }
                Ok(())
            },
        );

        // Nushell test case
        let nushell_test = create_test_case(
            "nushell",
            "echo 'Hello from Nushell! '",
            "Hello from Nushell! ",
            |executor| {
                if !Path::new(&executor.config.nu_bin).exists() {
                    executor.install_nu_shell(false).map_err(|e| format!("Nushell installation failed: {}", e))?;
                }
                Ok(())
            },
        );

        // Deno test case
        let deno_test = create_test_case(
            "deno",
            "console.log('Hello from Deno! ')",
            "Hello from Deno! ",
            |executor| {
                if !Path::new(&executor.config.deno_bin).exists() {
                    executor.install_deno(false).map_err(|e| format!("Deno installation failed: {}", e))?;
                }
                Ok(())
            },
        );

        // Execute all tests
        for test in [python_test, nushell_test, deno_test] {
            test_script_execution(test.clone())
                .unwrap_or_else(|e| panic!("{} test failed: {}", test.shell, e));
        }

        // Clean up environment (can be cleaned up again here)
        cleanup_test_environment().expect("Failed to clean up temporary directories");
    }

    #[test]
    fn test_install_choco() {
        let config = AgentConfig::default();
        let executor = ScriptExecutor::new(config);

        println!("=== Testing Chocolatey installation ===");
        match executor.install_choco() {
            Ok(result) => {
                assert!(result, "Chocolatey installation should return true on success");
                println!("Chocolatey installation test passed");
            },
            Err(e) => {
                panic!("Chocolatey installation failed: {}", e);
            }
        }

        // Verify Chocolatey installation
        let (stdout, stderr, exit_code) = executor.run_script(
            r#"
            $chocoPath = "$env:ProgramData\chocolatey\bin\choco.exe"
            if (Test-Path $chocoPath) {
                & $chocoPath --version
            } else {
                Write-Error "Chocolatey not found at expected path: $chocoPath"
                exit 1
            }
            "#,
            "powershell",
            vec![],
            30,
            false,
            vec![],
            false,
        ).expect("Failed to check Chocolatey version");

        println!("Chocolatey version check output: {}", stdout);
        println!("Chocolatey version check error: {}", stderr);
        assert_eq!(exit_code, 0, "Chocolatey version check should succeed");
        assert!(!stdout.trim().is_empty(), "Chocolatey version should not be empty");
    }
}