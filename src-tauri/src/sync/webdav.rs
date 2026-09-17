use anyhow::{anyhow, bail, Context};
use async_trait::async_trait;
use std::time::Duration;
use url::Url;

#[async_trait]
pub trait WebDavClient: Send + Sync {
    async fn ensure_root(&self) -> anyhow::Result<()>;
    /// 确保根目录及同步使用的固定子目录存在。
    async fn ensure_sync_dirs(&self) -> anyhow::Result<()> {
        self.ensure_root().await
    }
    async fn get(&self, path: &str) -> anyhow::Result<Option<Vec<u8>>>;
    /// 获取资源内容和 ETag；旧的测试/客户端实现可以退化为无 ETag。
    async fn get_with_etag(&self, path: &str) -> anyhow::Result<Option<(Vec<u8>, Option<String>)>> {
        Ok(self.get(path).await?.map(|bytes| (bytes, None)))
    }
    async fn put(&self, path: &str, bytes: Vec<u8>) -> anyhow::Result<()>;
    /// 条件写入 manifest，避免覆盖其他设备刚提交的版本。
    async fn put_manifest_conditional(
        &self,
        path: &str,
        bytes: Vec<u8>,
        etag: Option<&str>,
        expect_absent: bool,
    ) -> anyhow::Result<()> {
        let _ = (etag, expect_absent);
        self.put(path, bytes).await
    }
    async fn delete(&self, path: &str) -> anyhow::Result<()>;
}

pub struct ReqwestWebDavClient {
    client: reqwest::Client,
    base_url: Url,
    username: String,
    password: String,
}

fn webdav_method(bytes: &[u8]) -> reqwest::Method {
    reqwest::Method::from_bytes(bytes).expect("static WebDAV method")
}

impl ReqwestWebDavClient {
    pub fn new(
        server_url: &str,
        remote_dir: &str,
        username: &str,
        password: &str,
    ) -> anyhow::Result<Self> {
        let trimmed = server_url.trim().trim_end_matches('/');
        if trimmed.is_empty() {
            bail!("WebDAV 地址不能为空");
        }
        let mut base_url = Url::parse(trimmed).context("WebDAV 地址格式不正确")?;
        if base_url.scheme() != "http" && base_url.scheme() != "https" {
            bail!("WebDAV 地址必须使用 http 或 https");
        }
        if base_url.scheme() == "http" {
            let host = base_url.host_str().unwrap_or_default();
            let loopback = host.eq_ignore_ascii_case("localhost")
                || host.parse::<std::net::IpAddr>().map(|ip| ip.is_loopback()).unwrap_or(false);
            if !loopback {
                bail!("WebDAV 远端必须使用 HTTPS；HTTP 仅允许本机回环地址");
            }
        }
        if !base_url.username().is_empty() || base_url.password().is_some() {
            bail!("WebDAV 地址中不要内嵌用户名或密码");
        }
        let segments: Vec<&str> = remote_dir
            .split(['/', '\\'])
            .filter(|s| !s.is_empty())
            .collect();
        if segments.iter().any(|s| *s == "." || *s == "..") {
            bail!("远端目录不能包含 . 或 .. 路径片段");
        }
        {
            let mut path = base_url
                .path_segments_mut()
                .map_err(|_| anyhow!("WebDAV 地址无法作为目录使用"))?;
            path.pop_if_empty();
            for segment in segments {
                path.push(segment);
            }
            path.push("");
        }

        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(30))
            .timeout(Duration::from_secs(120))
            .build()
            .context("无法创建网络客户端")?;

        Ok(Self {
            client,
            base_url,
            username: username.trim().to_string(),
            password: password.to_string(),
        })
    }

    fn url_for(&self, relative: &str) -> Url {
        self.base_url
            .join(relative.trim_start_matches('/'))
            .unwrap_or_else(|_| self.base_url.clone())
    }

    fn request(&self, method: reqwest::Method, relative: &str) -> reqwest::RequestBuilder {
        self.client
            .request(method, self.url_for(relative))
            .basic_auth(&self.username, Some(&self.password))
    }

    async fn ensure_collection(&self, path: &str) -> anyhow::Result<()> {
        let response = self
            .request(webdav_method(b"MKCOL"), path)
            .send()
            .await
            .context("无法连接 WebDAV 服务")?;
        match response.status().as_u16() {
            200 | 201 | 204 | 405 => Ok(()),
            401 | 403 => Err(anyhow!("WebDAV 认证失败，请检查用户名和密码")),
            status => Err(anyhow!("无法创建同步目录: HTTP {}", status)),
        }
    }
}

#[async_trait]
impl WebDavClient for ReqwestWebDavClient {
    async fn ensure_root(&self) -> anyhow::Result<()> {
        self.ensure_collection("").await
    }

    async fn ensure_sync_dirs(&self) -> anyhow::Result<()> {
        for path in ["", "books/", "covers/", "progress/", "tombstones/"] {
            self.ensure_collection(path).await?;
        }
        Ok(())
    }

    async fn get(&self, path: &str) -> anyhow::Result<Option<Vec<u8>>> {
        Ok(self.get_with_etag(path).await?.map(|(bytes, _)| bytes))
    }

    async fn get_with_etag(&self, path: &str) -> anyhow::Result<Option<(Vec<u8>, Option<String>)>> {
        let response = self
            .request(reqwest::Method::GET, path)
            .send()
            .await
            .context("下载失败")?;
        match response.status().as_u16() {
            200 => {
                let etag = response
                    .headers()
                    .get(reqwest::header::ETAG)
                    .and_then(|value| value.to_str().ok())
                    .map(ToOwned::to_owned);
                let bytes = response.bytes().await.context("读取下载内容失败")?.to_vec();
                Ok(Some((bytes, etag)))
            }
            404 => Ok(None),
            401 | 403 => Err(anyhow!("WebDAV 认证失败，请检查用户名和密码")),
            status => Err(anyhow!("下载失败: HTTP {}", status)),
        }
    }

    async fn put(&self, path: &str, bytes: Vec<u8>) -> anyhow::Result<()> {
        if bytes.len() <= 128 * 1024 {
            return self.put_direct(path, &bytes).await;
        }

        let part_path = format!("{}.part", path.trim_start_matches('/'));
        if let Err(e) = self.put_direct(&part_path, &bytes).await {
            return Err(e.context("上传临时文件失败"));
        }

        let to = self.url_for(path);
        let response = self
            .request(webdav_method(b"MOVE"), &part_path)
            .header("Destination", to.as_str())
            .header("Overwrite", "T")
            .send()
            .await;

        match response {
            Ok(response) => match response.status().as_u16() {
                200 | 201 | 204 => {
                    let _ = self.request(reqwest::Method::DELETE, &part_path).send().await;
                    Ok(())
                }
                301 | 302 | 405 | 501 => {
                    let _ = self.delete(&part_path).await;
                    self.put_direct(path, &bytes).await
                }
                status => {
                    let _ = self.delete(&part_path).await;
                    Err(anyhow!("移动临时文件失败: HTTP {}", status))
                }
            },
            Err(e) => {
                let _ = self.delete(&part_path).await;
                self.put_direct(path, &bytes)
                    .await
                    .map_err(|direct| anyhow!("上传失败: {e}; 回退直传也失败: {direct}"))
            }
        }
    }

    async fn put_manifest_conditional(
        &self,
        path: &str,
        bytes: Vec<u8>,
        etag: Option<&str>,
        expect_absent: bool,
    ) -> anyhow::Result<()> {
        let mut request = self
            .request(reqwest::Method::PUT, path)
            .header("Overwrite", "T")
            .body(bytes);
        if let Some(etag) = etag {
            request = request.header("If-Match", etag);
        } else if expect_absent {
            request = request.header("If-None-Match", "*");
        } else {
            bail!("远端 manifest 没有 ETag，无法安全提交；请重新预览或更换支持 ETag 的服务");
        }
        let response = request.send().await.context("上传 manifest 失败")?;
        match response.status().as_u16() {
            200 | 201 | 204 => Ok(()),
            401 | 403 => Err(anyhow!("WebDAV 认证失败，请检查用户名和密码")),
            412 => Err(anyhow!("远端 manifest 已被其他设备更新，请重新预览同步")),
            status => Err(anyhow!("上传 manifest 失败: HTTP {}", status)),
        }
    }

    async fn delete(&self, path: &str) -> anyhow::Result<()> {
        let response = self
            .request(reqwest::Method::DELETE, path)
            .send()
            .await
            .context("删除远端文件失败")?;
        match response.status().as_u16() {
            200 | 202 | 204 | 404 => Ok(()),
            401 | 403 => Err(anyhow!("WebDAV 认证失败，请检查用户名和密码")),
            status => Err(anyhow!("删除远端文件失败: HTTP {}", status)),
        }
    }
}

impl ReqwestWebDavClient {
    async fn put_direct(&self, path: &str, bytes: &[u8]) -> anyhow::Result<()> {
        let response = self
            .request(reqwest::Method::PUT, path)
            .body(bytes.to_vec())
            .send()
            .await
            .context("上传失败")?;
        match response.status().as_u16() {
            200 | 201 | 204 => Ok(()),
            401 | 403 => Err(anyhow!("WebDAV 认证失败，请检查用户名和密码")),
            status => Err(anyhow!("上传失败: HTTP {}", status)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_traversal_remote_dir_segments() {
        let err = ReqwestWebDavClient::new("https://dav.example.com", "../escape", "u", "p")
            .err()
            .unwrap();
        assert!(err.to_string().contains(".."));

        let err = ReqwestWebDavClient::new("https://dav.example.com", "EpubReader/./x", "u", "p")
            .err()
            .unwrap();
        assert!(err.to_string().contains("."));
    }

    #[test]
    fn rejects_insecure_remote_http_but_allows_loopback() {
        let err = ReqwestWebDavClient::new("http://dav.example.com", "EpubReader", "u", "p")
            .err()
            .unwrap();
        assert!(err.to_string().contains("HTTPS"));
        assert!(ReqwestWebDavClient::new("http://127.0.0.1:8080", "EpubReader", "u", "p").is_ok());
    }

    #[test]
    fn builds_remote_url_from_base_and_remote_dir() {
        let client = ReqwestWebDavClient::new(
            "https://dav.example.com/remote.php/dav/files/user",
            "EpubReader",
            "u",
            "p",
        )
        .unwrap();
        assert_eq!(
            client.url_for("books/book-1.epub").as_str(),
            "https://dav.example.com/remote.php/dav/files/user/EpubReader/books/book-1.epub"
        );
    }
}
