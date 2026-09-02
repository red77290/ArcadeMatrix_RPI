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
    pub fn fetch_articles(
        api_key: &str,
        category: &str,
        keywords: &str,
        lang: &str,
        country: &str,
        max_articles: usize,
    ) -> Option<Vec<FetchedArticle>> {
        let key = api_key.trim();
        if key.is_empty() {
            return None;
        }

        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(5))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
            .build()
            .ok()?;

        let mut url = String::from("https://gnews.io/api/v4/");
        if !keywords.trim().is_empty() {
            let encoded_q = keywords.trim().replace(' ', "%20");
            url.push_str(&format!("search?q={}", encoded_q));
        } else {
            let cat = if category.is_empty() {
                "general"
            } else {
                category
            };
            url.push_str(&format!("top-headlines?category={}", cat));
        }

        if !lang.is_empty() && lang != "auto" {
            url.push_str(&format!("&lang={}", lang));
        }
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
                        if let Some(articles) = Self::parse_articles(&json, category) {
                            info!(
                                "[GNews API] Fetched {} live headlines for '{}'",
                                articles.len(),
                                category
                            );
                            return Some(articles);
                        }
                    }
                } else {
                    warn!("[GNews API] HTTP {} (429 = rate limited)", status.as_u16());
                }
            }
            Err(e) => {
                warn!("[GNews API] Request failed: {}", e);
            }
        }
        None
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
                title: title.to_string(),
                source: source.to_string(),
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
