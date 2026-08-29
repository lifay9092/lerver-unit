use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::AppError;

const ENV_FILE_NAME: &str = "environments.json";

/// 单个环境定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvItem {
    pub id: String,
    pub name: String,
    pub base_url: String,
}

/// 环境配置文件结构：environments.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvFile {
    /// 当前激活环境 id
    pub active: String,
    pub environments: Vec<EnvItem>,
}

impl Default for EnvFile {
    fn default() -> Self {
        Self {
            active: "dev".into(),
            environments: vec![
                EnvItem {
                    id: "dev".into(),
                    name: "开发环境".into(),
                    base_url: "http://127.0.0.1:8080".into(),
                },
                EnvItem {
                    id: "test".into(),
                    name: "测试环境".into(),
                    base_url: "https://test-api.example.com".into(),
                },
            ],
        }
    }
}

impl EnvFile {
    /// 当前激活环境的 baseUrl（找不到激活环境时返回空串）
    pub fn active_base_url(&self) -> &str {
        self.environments
            .iter()
            .find(|e| e.id == self.active)
            .map(|e| e.base_url.as_str())
            .unwrap_or("")
    }

    /// 当前激活环境名称
    pub fn active_name(&self) -> &str {
        self.environments
            .iter()
            .find(|e| e.id == self.active)
            .map(|e| e.name.as_str())
            .unwrap_or("")
    }
}

/// 读取环境配置；文件不存在时生成默认配置并写盘（FR-E1）
pub fn load_or_create(dir: &Path) -> Result<EnvFile, AppError> {
    let path = dir.join(ENV_FILE_NAME);
    if path.exists() {
        let text = std::fs::read_to_string(&path)?;
        let cfg: EnvFile = serde_json::from_str(&text)
            .map_err(|e| AppError::new(format!("environments.json 格式错误: {e}")))?;
        Ok(cfg)
    } else {
        let cfg = EnvFile::default();
        save(dir, &cfg)?;
        Ok(cfg)
    }
}

/// 写回环境配置文件
pub fn save(dir: &Path, cfg: &EnvFile) -> Result<(), AppError> {
    let path = dir.join(ENV_FILE_NAME);
    let text = serde_json::to_string_pretty(cfg)?;
    std::fs::write(path, text)?;
    Ok(())
}

/// URL 解析（FR-E2）：
/// 1. 以 http:// 或 https:// 开头 → 原样返回
/// 2. 含 {{baseUrl}} 占位符 → 替换为当前环境 baseUrl
/// 3. 其余（相对路径）→ 拼接当前环境 baseUrl
/// 需要拼接/替换但 baseUrl 为空时返回错误
pub fn resolve_url(url: &str, base_url: &str) -> Result<String, String> {
    let u = url.trim();
    if u.is_empty() {
        return Err("URL 不能为空".into());
    }
    if u.starts_with("http://") || u.starts_with("https://") {
        return Ok(u.to_string());
    }
    let base = base_url.trim();
    if base.is_empty() {
        return Err(format!("用例 URL「{u}」需要 baseUrl，但当前环境未配置 baseUrl"));
    }
    let base = base.trim_end_matches('/');
    if u.contains("{{baseUrl}}") {
        const TAG: &str = "{{baseUrl}}";
        let mut result = String::new();
        let mut rest = u;
        while let Some(pos) = rest.find(TAG) {
            result.push_str(&rest[..pos]);
            // 占位符前需要分隔符
            if !result.is_empty() && !result.ends_with('/') {
                result.push('/');
            }
            result.push_str(base);
            let after = &rest[pos + TAG.len()..];
            // 占位符后需要分隔符
            if !after.is_empty() && !after.starts_with('/') {
                result.push('/');
            }
            rest = after;
        }
        result.push_str(rest);
        Ok(result)
    } else {
        Ok(format!("{}/{}", base, u.trim_start_matches('/')))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "http://192.168.1.10:8080";

    #[test]
    fn absolute_url_passes_through() {
        assert_eq!(
            resolve_url("https://api.example.com/login?a=1", BASE).unwrap(),
            "https://api.example.com/login?a=1"
        );
        assert_eq!(resolve_url("http://x.com", "").unwrap(), "http://x.com");
    }

    #[test]
    fn relative_path_joins_base() {
        assert_eq!(resolve_url("/api/login", BASE).unwrap(), "http://192.168.1.10:8080/api/login");
        // base 带尾斜杠
        assert_eq!(
            resolve_url("/api/login", "http://192.168.1.10:8080/").unwrap(),
            "http://192.168.1.10:8080/api/login"
        );
        // 多级相对路径
        assert_eq!(
            resolve_url("api/v1/user/1", BASE).unwrap(),
            "http://192.168.1.10:8080/api/v1/user/1"
        );
    }

    #[test]
    fn placeholder_replaces_base() {
        assert_eq!(
            resolve_url("{{baseUrl}}/api/login", BASE).unwrap(),
            "http://192.168.1.10:8080/api/login"
        );
        assert_eq!(
            resolve_url("{{baseUrl}}api/login", "https://t.example.com/").unwrap(),
            "https://t.example.com/api/login"
        );
    }

    #[test]
    fn empty_base_errors() {
        assert!(resolve_url("/api/login", "").is_err());
        assert!(resolve_url("{{baseUrl}}/api/login", "").is_err());
        // 绝对地址不受空 base 影响
        assert!(resolve_url("https://a.com/x", "").is_ok());
    }

    #[test]
    fn empty_url_errors() {
        assert!(resolve_url("  ", BASE).is_err());
    }
}
