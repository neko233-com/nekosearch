use async_trait::async_trait;
use nekosearch_core::{CrawlResult, CrawlTask};
use reqwest::Client;

use crate::crawler::executor::CrawlerExecutor;

/// 基于 HTTP 的网页爬虫执行器（示例）。
pub struct HttpCrawler {
    client: Client,
}

impl HttpCrawler {
    pub fn new() -> Self {
        Self { client: Client::new() }
    }
}

#[async_trait]
impl CrawlerExecutor for HttpCrawler {
    fn name(&self) -> &'static str {
        "http"
    }

    async fn crawl(&self, task: &CrawlTask) -> anyhow::Result<CrawlResult> {
        let body = self.client.get(&task.url).send().await?.text().await?;
        let title = extract_title(&body).unwrap_or_else(|| task.url.clone());
        let text = strip_tags(&body);
        let links = extract_links(&body, &task.url);
        Ok(CrawlResult {
            task_id: task.id.clone(),
            url: task.url.clone(),
            title,
            text,
            links,
        })
    }
}

/// 从 HTML 中抽取 <title> 内容。
fn extract_title(html: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let start = lower.find("<title")?;
    let after_tag = html[start..].find('>').map(|i| start + i + 1)?;
    let end = lower[after_tag..].find("</title>").map(|i| after_tag + i)?;
    let t = html[after_tag..end].trim().to_string();
    if t.is_empty() {
        None
    } else {
        Some(t)
    }
}

/// 去除 HTML 标签并压缩空白。
pub fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 从 HTML 中抽取外链并相对当前页面解析为绝对地址。
pub fn extract_links(html: &str, base: &str) -> Vec<String> {
    let base_url = url::Url::parse(base).ok();
    let mut links = Vec::new();
    let bytes = html.as_bytes();
    let mut i = 0;
    while let Some(pos) = html[i..].find("href") {
        let idx = i + pos;
        let mut j = idx + 4;
        while j < html.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
            j += 1;
        }
        if j >= html.len() || bytes[j] != b'=' {
            i = idx + 4;
            continue;
        }
        j += 1;
        while j < html.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
            j += 1;
        }
        if j >= html.len() {
            break;
        }
        let (quote, close) = if bytes[j] == b'"' {
            (b'"', b'"')
        } else if bytes[j] == b'\'' {
            (b'\'', b'\'')
        } else {
            (b' ', b'>')
        };
        let start = if quote == b' ' { j } else { j + 1 };
        let mut end = start;
        while end < html.len() && bytes[end] != close {
            end += 1;
        }
        let link = &html[start..end];
        if !link.is_empty() {
            if let Some(resolved) = resolve(base_url.as_ref(), link) {
                links.push(resolved);
            }
        }
        i = end + 1;
    }
    links
}

fn resolve(base: Option<&url::Url>, link: &str) -> Option<String> {
    if link.starts_with("http://")
        || link.starts_with("https://")
        || link.starts_with("file://")
    {
        return Some(link.to_string());
    }
    if let Some(b) = base {
        if let Ok(joined) = b.join(link) {
            return Some(joined.to_string());
        }
    }
    None
}
