use crate::core::i18n::GNewsStatus;
use std::time::Duration;
use tracing::{info, warn};

#[derive(Clone, Debug)]
pub struct FetchedArticle {
    pub title: String,
    pub source: String,
    pub category: String,
    pub published_epoch: u64,
}

pub struct GNewsProvider;

impl GNewsProvider {
    pub fn fetch_articles_multi_key(
        keys: &[String],
        active_key_idx: &mut usize,
        key_usages: &mut Vec<u32>,
        category: &str,
        keywords: &str,
        lang: &str,
        country: &str,
        max_articles: usize,
    ) -> (GNewsStatus, Option<Vec<FetchedArticle>>) {
        if keys.is_empty() {
            return (GNewsStatus::EmptyKey, None);
        }

        while key_usages.len() < keys.len() {
            key_usages.push(0);
        }

        let client = match reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(6))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
            .build()
        {
            Ok(c) => c,
            Err(_) => return (GNewsStatus::NetworkError, None),
        };

        let start_idx = *active_key_idx % keys.len();
        let mut last_status = GNewsStatus::EmptyKey;

        for attempt in 0..keys.len() {
            let cur_idx = (start_idx + attempt) % keys.len();
            let key = keys[cur_idx].trim();
            if key.is_empty() {
                continue;
            }

            let (status, arts_opt) = Self::fetch_single_category(
                &client,
                key,
                category,
                keywords,
                lang,
                country,
                max_articles,
            );

            last_status = status;
            if status == GNewsStatus::Ok && arts_opt.is_some() {
                *active_key_idx = cur_idx;
                key_usages[cur_idx] = key_usages[cur_idx].saturating_add(1);
                info!(
                    "[GNews API] Query succeeded with key {}/{} (used: {} reqs today)",
                    cur_idx + 1,
                    keys.len(),
                    key_usages[cur_idx]
                );
                return (GNewsStatus::Ok, arts_opt);
            }

            warn!(
                "[GNews API] Key {}/{} failed with status {:?}. Attempting failover to next key...",
                cur_idx + 1,
                keys.len(),
                status
            );
        }

        (last_status, None)
    }

    pub fn fetch_articles(
        api_key: &str,
        category: &str,
        keywords: &str,
        lang: &str,
        country: &str,
        max_articles: usize,
    ) -> (GNewsStatus, Option<Vec<FetchedArticle>>) {
        let keys: Vec<String> = api_key
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let mut active_idx = 0;
        let mut usages = vec![0; keys.len()];
        Self::fetch_articles_multi_key(
            &keys,
            &mut active_idx,
            &mut usages,
            category,
            keywords,
            lang,
            country,
            max_articles,
        )
    }

    fn fetch_single_category(
        client: &reqwest::blocking::Client,
        key: &str,
        cat: &str,
        keywords: &str,
        lang: &str,
        country: &str,
        max_articles: usize,
    ) -> (GNewsStatus, Option<Vec<FetchedArticle>>) {
        let mut url = String::from("https://gnews.io/api/v4/");
        if !keywords.trim().is_empty() {
            let encoded_q = keywords.trim().replace(' ', "%20");
            url.push_str(&format!("search?q={}", encoded_q));
        } else {
            let c = if cat.is_empty() { "general" } else { cat };
            url.push_str(&format!("top-headlines?category={}", c));
        }

        let lang_lower = lang.trim().to_lowercase();
        let effective_lang = match lang_lower.as_str() {
            "" | "auto" | "system" => "fr",
            other => other,
        };
        url.push_str(&format!("&lang={}", effective_lang));

        if !country.is_empty() && country != "auto" {
            url.push_str(&format!("&country={}", country));
        }
        let count = max_articles.clamp(1, 10);
        url.push_str(&format!("&max={}&apikey={}", count, key));

        match client.get(&url).send() {
            Ok(res) => {
                let status = res.status();
                if status.is_success() {
                    if let Ok(json) = res.json::<serde_json::Value>() {
                        if let Some(articles) = Self::parse_articles(&json, cat) {
                            info!(
                                "[GNews API] Fetched {} live headlines for '{}' (lang={})",
                                articles.len(),
                                cat,
                                effective_lang
                            );
                            return (GNewsStatus::Ok, Some(articles));
                        }
                    }
                    (GNewsStatus::Ok, None)
                } else {
                    let err_body = res.text().unwrap_or_default();
                    let err_lower = err_body.to_lowercase();
                    if status.as_u16() == 429
                        || (status.as_u16() == 403
                            && (err_lower.contains("consumed")
                                || err_lower.contains("quota")
                                || err_lower.contains("limit")
                                || err_lower.contains("plan")
                                || err_lower.contains("requests")))
                    {
                        warn!(
                            "[GNews API] Rate limit / daily quota reached (HTTP {}): {}",
                            status.as_u16(),
                            err_body
                        );
                        (GNewsStatus::RateLimited, None)
                    } else if status.as_u16() == 401
                        || err_lower.contains("invalid")
                        || err_lower.contains("forbidden")
                    {
                        warn!(
                            "[GNews API] Invalid API key (HTTP {}): {}",
                            status.as_u16(),
                            err_body
                        );
                        (GNewsStatus::InvalidKey, None)
                    } else {
                        warn!("[GNews API] HTTP error {}: {}", status.as_u16(), err_body);
                        (GNewsStatus::NetworkError, None)
                    }
                }
            }
            Err(e) => {
                warn!("[GNews API] Request failed: {}", e);
                (GNewsStatus::NetworkError, None)
            }
        }
    }

    pub fn clean_text(raw: &str) -> String {
        raw.replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&#39;", "'")
            .replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&nbsp;", " ")
            .replace("&#8217;", "'")
            .replace("&#8216;", "'")
            .replace("&#8220;", "\"")
            .replace("&#8221;", "\"")
            .replace("&#8211;", "-")
            .replace("&#8212;", "-")
            .trim()
            .to_string()
    }

    pub fn save_cache_file(
        path: &str,
        articles: &[FetchedArticle],
        last_fetch_epoch: u64,
        last_cat_idx: usize,
        active_key_idx: usize,
        key_usages: &[u32],
        status: u8,
    ) {
        let json_data = serde_json::json!({
            "last_fetch_epoch": last_fetch_epoch,
            "last_cat_idx": last_cat_idx,
            "active_key_idx": active_key_idx,
            "key_usages": key_usages,
            "status": status,
            "articles": articles.iter().map(|a| serde_json::json!({
                "title": a.title,
                "source": a.source,
                "category": a.category,
                "published_epoch": a.published_epoch,
            })).collect::<Vec<_>>()
        });

        if let Ok(serialized) = serde_json::to_string_pretty(&json_data) {
            let _ = std::fs::write(path, serialized);
        }
    }

    pub fn load_cache_file(
        path: &str,
    ) -> Option<(Vec<FetchedArticle>, u64, usize, usize, Vec<u32>, u8)> {
        let content = std::fs::read_to_string(path).ok()?;
        let json: serde_json::Value = serde_json::from_str(&content).ok()?;

        let last_fetch_epoch = json.get("last_fetch_epoch")?.as_u64().unwrap_or(0);
        let last_cat_idx = json.get("last_cat_idx")?.as_u64().unwrap_or(0) as usize;
        let active_key_idx = json
            .get("active_key_idx")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let status = json.get("status")?.as_u64().unwrap_or(0) as u8;

        let key_usages = json
            .get("key_usages")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_u64().map(|u| u as u32))
                    .collect()
            })
            .unwrap_or_default();

        let articles_arr = json.get("articles")?.as_array()?;
        let mut list = Vec::new();
        for item in articles_arr {
            let title = item.get("title")?.as_str()?.to_string();
            let source = item.get("source")?.as_str()?.to_string();
            let category = item.get("category")?.as_str()?.to_string();
            let published_epoch = item
                .get("published_epoch")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            list.push(FetchedArticle {
                title,
                source,
                category,
                published_epoch,
            });
        }
        Some((
            list,
            last_fetch_epoch,
            last_cat_idx,
            active_key_idx,
            key_usages,
            status,
        ))
    }

    pub fn parse_articles(
        json: &serde_json::Value,
        default_category: &str,
    ) -> Option<Vec<FetchedArticle>> {
        let arr = json.get("articles")?.as_array()?;
        let mut list = Vec::new();
        let def_cat = if default_category.is_empty() {
            "News"
        } else {
            default_category
        };

        for item in arr {
            let title = item.get("title").and_then(|v| v.as_str())?;
            if title.is_empty() {
                continue;
            }
            let source = item
                .get("source")
                .and_then(|s| s.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("News");

            list.push(FetchedArticle {
                title: Self::clean_text(title),
                source: Self::clean_text(source),
                category: def_cat.to_string(),
                published_epoch: 0,
            });
        }

        if list.is_empty() {
            None
        } else {
            Some(list)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_gnews_json() {
        let sample = serde_json::json!({
            "totalArticles": 2,
            "articles": [
                {
                    "title": "Quantum Computing Milestone",
                    "source": { "name": "TechCrunch" },
                    "publishedAt": "2026-09-02T08:00:00Z"
                },
                {
                    "title": "New Telescope Discovery",
                    "source": { "name": "Nature" },
                    "publishedAt": "2026-09-02T08:15:00Z"
                }
            ]
        });

        let parsed = GNewsProvider::parse_articles(&sample, "technology").unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].title, "Quantum Computing Milestone");
        assert_eq!(parsed[0].source, "TechCrunch");
        assert_eq!(parsed[0].category, "technology");
    }
}
