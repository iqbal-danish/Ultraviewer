use quick_xml::events::Event;
use quick_xml::reader::Reader;
use std::collections::HashSet;
use crate::formats::FileType;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestionKind {
    Element,
    Attribute,
    Path,
    Template,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SuggestionCategory {
    #[default]
    All,
    BidsAndPricing,
    LocationAndGeo,
    LinksAndQuality,
    EmptyFields,
    HasData,
    Elements,
    Attributes,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuerySuggestion {
    pub label: String,
    pub description: String,
    pub query: String,
    pub kind: SuggestionKind,
    pub category: SuggestionCategory,
}

pub fn is_pricing_tag(tag: &str) -> bool {
    let lower = tag.to_lowercase();
    lower.contains("cpc")
        || lower.contains("bid")
        || lower.contains("budget")
        || lower.contains("cost")
        || lower.contains("price")
        || lower.contains("rate")
        || lower.contains("salary")
        || lower.contains("wage")
        || lower.contains("compensation")
        || lower.contains("payout")
}

pub fn is_location_tag(tag: &str) -> bool {
    let lower = tag.to_lowercase();
    lower.contains("state")
        || lower.contains("country")
        || lower.contains("city")
        || lower.contains("location")
        || lower.contains("zip")
        || lower.contains("postal")
        || lower.contains("region")
        || lower.contains("geo")
        || lower.contains("address")
        || lower.contains("nation")
}

pub fn is_link_tag(tag: &str) -> bool {
    let lower = tag.to_lowercase();
    lower.contains("url")
        || lower.contains("link")
        || lower.contains("apply")
        || lower.contains("href")
        || lower.contains("source")
        || lower.contains("landing")
        || lower.contains("click")
        || lower.contains("redirect")
}

pub fn is_content_tag(tag: &str) -> bool {
    let lower = tag.to_lowercase();
    lower.contains("title")
        || lower.contains("description")
        || lower.contains("body")
        || lower.contains("summary")
        || lower.contains("headline")
        || lower.contains("content")
        || lower.contains("text")
}

#[derive(Debug, Clone, Default)]
pub struct DiscoveredTags {
    pub tags: Vec<String>,
    pub attributes: Vec<String>,
    pub root_tag: Option<String>,
    pub record_tag: Option<String>,
}

impl DiscoveredTags {
    /// Discover unique tags, attributes, and record structures from a file sample (< 1ms).
    pub fn from_sample(sample: &[u8], file_type: Option<FileType>) -> Self {
        match file_type {
            Some(FileType::Xml) => Self::discover_xml(sample),
            Some(FileType::Json) => Self::discover_json(sample),
            _ => Self::default(),
        }
    }

    fn discover_xml(sample: &[u8]) -> Self {
        let mut reader = Reader::from_reader(sample);
        reader.config_mut().trim_text(true);
        reader.config_mut().check_end_names = false;

        let mut tags_seen = HashSet::new();
        let mut tags = Vec::new();
        let mut attrs_seen = HashSet::new();
        let mut attributes = Vec::new();
        let mut root_tag = None;
        let mut tag_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        let mut container_tags: HashSet<String> = HashSet::new();
        let mut tag_stack: Vec<String> = Vec::new();

        let mut buf = Vec::with_capacity(512);
        let mut event_count = 0;

        loop {
            if event_count > 4000 {
                break;
            }
            event_count += 1;

            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) => {
                    let raw_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    let clean_name = raw_name.split(':').last().unwrap_or(&raw_name).to_string();

                    if !clean_name.is_empty() && !clean_name.starts_with('?') && !clean_name.starts_with('!') {
                        if root_tag.is_none() {
                            root_tag = Some(clean_name.clone());
                        }

                        if let Some(parent) = tag_stack.last() {
                            container_tags.insert(parent.clone());
                        }
                        tag_stack.push(clean_name.clone());

                        *tag_counts.entry(clean_name.clone()).or_insert(0) += 1;

                        if tags_seen.insert(clean_name.clone()) {
                            tags.push(clean_name);
                        }

                        for attr in e.attributes().flatten() {
                            let raw_key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                            let clean_key = raw_key.split(':').last().unwrap_or(&raw_key).to_string();
                            if !clean_key.is_empty() && !clean_key.starts_with("xmlns") && attrs_seen.insert(clean_key.clone()) {
                                attributes.push(clean_key);
                            }
                        }
                    }
                }
                Ok(Event::End(_)) => {
                    tag_stack.pop();
                }
                Ok(Event::Empty(e)) => {
                    let raw_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    let clean_name = raw_name.split(':').last().unwrap_or(&raw_name).to_string();

                    if !clean_name.is_empty() && !clean_name.starts_with('?') && !clean_name.starts_with('!') {
                        if root_tag.is_none() {
                            root_tag = Some(clean_name.clone());
                        }

                        if let Some(parent) = tag_stack.last() {
                            container_tags.insert(parent.clone());
                        }

                        *tag_counts.entry(clean_name.clone()).or_insert(0) += 1;

                        if tags_seen.insert(clean_name.clone()) {
                            tags.push(clean_name);
                        }

                        for attr in e.attributes().flatten() {
                            let raw_key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                            let clean_key = raw_key.split(':').last().unwrap_or(&raw_key).to_string();
                            if !clean_key.is_empty() && !clean_key.starts_with("xmlns") && attrs_seen.insert(clean_key.clone()) {
                                attributes.push(clean_key);
                            }
                        }
                    }
                }
                Ok(Event::Eof) | Err(_) => break,
                _ => {}
            }
            buf.clear();
        }

        // Prioritize container tags (tags that contain child elements, like <job>)
        // over leaf tags (like <refcode>, <title>) which may have equal counts.
        let record_tag = tag_counts
            .iter()
            .filter(|(k, _)| root_tag.as_ref().map(|r| r != *k).unwrap_or(true))
            .filter(|(k, _)| container_tags.contains(*k))
            .max_by_key(|(_, count)| *count)
            .map(|(k, _)| k.clone())
            .or_else(|| {
                tag_counts
                    .iter()
                    .filter(|(k, _)| root_tag.as_ref().map(|r| r != *k).unwrap_or(true))
                    .max_by_key(|(_, count)| *count)
                    .map(|(k, _)| k.clone())
            });

        Self {
            tags,
            attributes,
            root_tag,
            record_tag,
        }
    }

    fn discover_json(sample: &[u8]) -> Self {
        let mut keys_seen = HashSet::new();
        let mut tags = Vec::new();

        if let Ok(text) = std::str::from_utf8(sample) {
            for line in text.lines().take(500) {
                let trimmed = line.trim();
                if let Some(pos) = trimmed.find("\":") {
                    let before = &trimmed[..pos];
                    if let Some(start) = before.rfind('"') {
                        let key = before[start + 1..].trim();
                        if !key.is_empty() && keys_seen.insert(key.to_string()) {
                            tags.push(key.to_string());
                        }
                    }
                }
            }
        }

        Self {
            tags,
            attributes: Vec::new(),
            root_tag: None,
            record_tag: None,
        }
    }

    /// Merge tags observed from active breadcrumb paths as user navigates
    pub fn observe_path(&mut self, path: &str) {
        let is_xml = path.starts_with('/') || !path.starts_with('$');
        let segments: Vec<&str> = if is_xml {
            path.split('/').filter(|s| !s.is_empty()).collect()
        } else {
            path.split('.').filter(|s| !s.is_empty()).collect()
        };

        for seg in segments {
            let clean = seg.split('[').next().unwrap_or(seg).trim();
            if clean.starts_with('@') {
                let attr = clean.trim_start_matches('@').to_string();
                if !attr.is_empty() && !self.attributes.contains(&attr) {
                    self.attributes.push(attr);
                }
            } else if !clean.is_empty() && !self.tags.contains(&clean.to_string()) {
                self.tags.push(clean.to_string());
            }
        }
    }

    /// Generate categorized smart query suggestions based on user input and active path
    pub fn suggest(
        &self,
        input: &str,
        current_path: Option<&str>,
        file_type: Option<FileType>,
    ) -> Vec<QuerySuggestion> {
        let is_xml = matches!(file_type, Some(FileType::Xml)) || (file_type.is_none() && !input.starts_with('$'));
        let trimmed = input.trim();

        if is_xml {
            self.suggest_xml(trimmed, current_path)
        } else {
            self.suggest_json(trimmed)
        }
    }

    fn suggest_xml(&self, input: &str, current_path: Option<&str>) -> Vec<QuerySuggestion> {
        let mut results = Vec::new();
        let query_lower = input.to_lowercase();

        // Extract clean path segments:
        // Correctly handles paths with indices and predicates like /jobfeed/jobs/job[2]/title or //cpc[not(text())]
        let path_segments: Vec<String> = input
            .split('/')
            .filter(|s| !s.is_empty())
            .map(|s| {
                s.split('[')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .trim_start_matches('@')
                    .to_string()
            })
            .filter(|s| !s.is_empty())
            .collect();

        let leaf_from_path = path_segments.last().map(|s| s.as_str()).unwrap_or("");
        let parent_from_path = if path_segments.len() >= 2 {
            let p = &path_segments[path_segments.len() - 2];
            if !p.is_empty() { Some(p.as_str()) } else { None }
        } else {
            None
        };

        let raw_term = if !leaf_from_path.is_empty() && (input.contains('/') || input.contains('[')) {
            leaf_from_path.to_string()
        } else {
            query_lower
                .trim_start_matches("//")
                .trim_start_matches('/')
                .trim_start_matches('@')
                .trim_start_matches('[')
                .to_string()
        };
        let term = raw_term.to_lowercase();

        let is_empty_query = term == "empty" || term == "blank" || term == "missing" || term == "null";

        // Extract active breadcrumb leaf tag and parent tag if available
        let mut active_tag: Option<String> = None;
        let mut active_parent: Option<String> = None;
        if let Some(path) = current_path {
            let segments: Vec<String> = path
                .split('/')
                .filter(|s| !s.is_empty())
                .map(|s| s.split('[').next().unwrap_or("").trim().trim_start_matches('@').to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if !segments.is_empty() {
                active_tag = Some(segments[segments.len() - 1].clone());
                if segments.len() >= 2 {
                    active_parent = Some(segments[segments.len() - 2].clone());
                }
            }
        }

        let rec_tag = self.record_tag.as_deref()
            .or(parent_from_path)
            .or(active_parent.as_deref())
            .unwrap_or("item");

        let push_empty_variants_for_tag = |results: &mut Vec<QuerySuggestion>, tag: &str, rec: &str| {
            if tag == rec {
                return;
            }
            results.push(QuerySuggestion {
                label: format!("//{}[not(normalize-space({}))]", rec, tag),
                description: format!("Find <{}> where <{}> is empty, blank, or missing", rec, tag),
                query: format!("//{}[not(normalize-space({}))]", rec, tag),
                kind: SuggestionKind::Template,
                category: SuggestionCategory::EmptyFields,
            });
            results.push(QuerySuggestion {
                label: format!("//{}[not(text())]", tag),
                description: format!("Directly find all empty <{}/> or <{}></{}> tags", tag, tag, tag),
                query: format!("//{}[not(text())]", tag),
                kind: SuggestionKind::Template,
                category: SuggestionCategory::EmptyFields,
            });
            results.push(QuerySuggestion {
                label: format!("//{}[normalize-space()='']", tag),
                description: format!("Find all <{}> tags containing blank whitespace or empty", tag),
                query: format!("//{}[normalize-space()='']", tag),
                kind: SuggestionKind::Template,
                category: SuggestionCategory::EmptyFields,
            });
            results.push(QuerySuggestion {
                label: format!("//{}[not({})]", rec, tag),
                description: format!("Find <{}> where <{}> tag is completely missing", rec, tag),
                query: format!("//{}[not({})]", rec, tag),
                kind: SuggestionKind::Template,
                category: SuggestionCategory::EmptyFields,
            });
            results.push(QuerySuggestion {
                label: format!("//{}[{}='']", rec, tag),
                description: format!("Find <{}> where <{}> is explicitly empty ''", rec, tag),
                query: format!("//{}[{}='']", rec, tag),
                kind: SuggestionKind::Template,
                category: SuggestionCategory::EmptyFields,
            });
        };

        let push_pricing_variants_for_tag = |results: &mut Vec<QuerySuggestion>, tag: &str, rec: &str| {
            if tag == rec {
                return;
            }
            results.push(QuerySuggestion {
                label: format!("//{}[{}<=0]", rec, tag),
                description: format!("Find <{}> where <{}> <= 0 (invalid/missing/zero bid)", rec, tag),
                query: format!("//{}[{}<=0]", rec, tag),
                kind: SuggestionKind::Template,
                category: SuggestionCategory::BidsAndPricing,
            });
            results.push(QuerySuggestion {
                label: format!("//{}[{}>2.0]", rec, tag),
                description: format!("Find high-bid <{}> where <{}> > $2.00", rec, tag),
                query: format!("//{}[{}>2.0]", rec, tag),
                kind: SuggestionKind::Template,
                category: SuggestionCategory::BidsAndPricing,
            });
            results.push(QuerySuggestion {
                label: format!("//{}[{}<0.50]", rec, tag),
                description: format!("Find low-bid <{}> where <{}> < $0.50", rec, tag),
                query: format!("//{}[{}<0.50]", rec, tag),
                kind: SuggestionKind::Template,
                category: SuggestionCategory::BidsAndPricing,
            });
            results.push(QuerySuggestion {
                label: format!("//{}[{}>5.0]", rec, tag),
                description: format!("Find premium-tier <{}> where <{}> > $5.00", rec, tag),
                query: format!("//{}[{}>5.0]", rec, tag),
                kind: SuggestionKind::Template,
                category: SuggestionCategory::BidsAndPricing,
            });
            results.push(QuerySuggestion {
                label: format!("//{}[{}=0]", rec, tag),
                description: format!("Find free clicks <{}> where <{}> = 0", rec, tag),
                query: format!("//{}[{}=0]", rec, tag),
                kind: SuggestionKind::Template,
                category: SuggestionCategory::BidsAndPricing,
            });
        };

        let push_location_variants_for_tag = |results: &mut Vec<QuerySuggestion>, tag: &str, rec: &str| {
            if tag == rec {
                return;
            }
            let lower = tag.to_lowercase();
            if lower.contains("country") || lower.contains("nation") {
                results.push(QuerySuggestion {
                    label: format!("//{}[{}!='US']", rec, tag),
                    description: format!("Find international / non-US <{}> (<{}> != 'US')", rec, tag),
                    query: format!("//{}[{}!='US']", rec, tag),
                    kind: SuggestionKind::Template,
                    category: SuggestionCategory::LocationAndGeo,
                });
                results.push(QuerySuggestion {
                    label: format!("//{}[{}='US']", rec, tag),
                    description: format!("Find US domestic <{}> (<{}> = 'US')", rec, tag),
                    query: format!("//{}[{}='US']", rec, tag),
                    kind: SuggestionKind::Template,
                    category: SuggestionCategory::LocationAndGeo,
                });
                results.push(QuerySuggestion {
                    label: format!("//{}[contains({}, 'Remote')]", rec, tag),
                    description: format!("Find Remote <{}> via <{}>", rec, tag),
                    query: format!("//{}[contains({}, 'Remote')]", rec, tag),
                    kind: SuggestionKind::Template,
                    category: SuggestionCategory::LocationAndGeo,
                });
            } else if lower.contains("state") || lower.contains("region") {
                results.push(QuerySuggestion {
                    label: format!("//{}[{}='CA']", rec, tag),
                    description: format!("Find California <{}> (<{}> = 'CA')", rec, tag),
                    query: format!("//{}[{}='CA']", rec, tag),
                    kind: SuggestionKind::Template,
                    category: SuggestionCategory::LocationAndGeo,
                });
                results.push(QuerySuggestion {
                    label: format!("//{}[{}='TX']", rec, tag),
                    description: format!("Find Texas <{}> (<{}> = 'TX')", rec, tag),
                    query: format!("//{}[{}='TX']", rec, tag),
                    kind: SuggestionKind::Template,
                    category: SuggestionCategory::LocationAndGeo,
                });
                results.push(QuerySuggestion {
                    label: format!("//{}[string-length({})!=2]", rec, tag),
                    description: format!("Find non-standard state codes in <{}> (length != 2)", tag),
                    query: format!("//{}[string-length({})!=2]", rec, tag),
                    kind: SuggestionKind::Template,
                    category: SuggestionCategory::LocationAndGeo,
                });
            } else if lower.contains("zip") || lower.contains("postal") {
                results.push(QuerySuggestion {
                    label: format!("//{}[string-length({})<5]", rec, tag),
                    description: format!("Find truncated/malformed postal codes in <{}>", tag),
                    query: format!("//{}[string-length({})<5]", rec, tag),
                    kind: SuggestionKind::Template,
                    category: SuggestionCategory::LocationAndGeo,
                });
            } else {
                results.push(QuerySuggestion {
                    label: format!("//{}[contains({}, 'Remote')]", rec, tag),
                    description: format!("Find Remote <{}> in <{}>", rec, tag),
                    query: format!("//{}[contains({}, 'Remote')]", rec, tag),
                    kind: SuggestionKind::Template,
                    category: SuggestionCategory::LocationAndGeo,
                });
                results.push(QuerySuggestion {
                    label: format!("//{}[string-length({})<3]", rec, tag),
                    description: format!("Find suspiciously short location strings in <{}>", tag),
                    query: format!("//{}[string-length({})<3]", rec, tag),
                    kind: SuggestionKind::Template,
                    category: SuggestionCategory::LocationAndGeo,
                });
            }
        };

        let push_link_variants_for_tag = |results: &mut Vec<QuerySuggestion>, tag: &str, rec: &str| {
            if tag == rec {
                return;
            }
            results.push(QuerySuggestion {
                label: format!("//{}[not(starts-with({}, 'https://'))]", rec, tag),
                description: format!("Flag unsecure or non-HTTPS URLs in <{}>", tag),
                query: format!("//{}[not(starts-with({}, 'https://'))]", rec, tag),
                kind: SuggestionKind::Template,
                category: SuggestionCategory::LinksAndQuality,
            });
            results.push(QuerySuggestion {
                label: format!("//{}[not(contains({}, 'utm_source'))]", rec, tag),
                description: format!("Find URLs in <{}> missing UTM campaign tracking", tag),
                query: format!("//{}[not(contains({}, 'utm_source'))]", rec, tag),
                kind: SuggestionKind::Template,
                category: SuggestionCategory::LinksAndQuality,
            });
            results.push(QuerySuggestion {
                label: format!("//{}[contains({}, 'localhost')]", rec, tag),
                description: format!("Flag staging/localhost links leaking in <{}>", tag),
                query: format!("//{}[contains({}, 'localhost')]", rec, tag),
                kind: SuggestionKind::Template,
                category: SuggestionCategory::LinksAndQuality,
            });
            results.push(QuerySuggestion {
                label: format!("//{}[string-length({})<15]", rec, tag),
                description: format!("Flag broken or truncated URLs in <{}>", tag),
                query: format!("//{}[string-length({})<15]", rec, tag),
                kind: SuggestionKind::Template,
                category: SuggestionCategory::LinksAndQuality,
            });
        };

        let push_content_quality_variants_for_tag = |results: &mut Vec<QuerySuggestion>, tag: &str, rec: &str| {
            if tag == rec {
                return;
            }
            results.push(QuerySuggestion {
                label: format!("//{}[string-length({})<5]", rec, tag),
                description: format!("Find suspiciously short <{}> (< 5 chars)", tag),
                query: format!("//{}[string-length({})<5]", rec, tag),
                kind: SuggestionKind::Template,
                category: SuggestionCategory::LinksAndQuality,
            });
            results.push(QuerySuggestion {
                label: format!("//{}[contains({}, 'Test')]", rec, tag),
                description: format!("Find test jobs/records with 'Test' in <{}>", tag),
                query: format!("//{}[contains({}, 'Test')]", rec, tag),
                kind: SuggestionKind::Template,
                category: SuggestionCategory::LinksAndQuality,
            });
            results.push(QuerySuggestion {
                label: format!("//{}[contains({}, '&amp;')]", rec, tag),
                description: format!("Find unescaped HTML entities in <{}>", tag),
                query: format!("//{}[contains({}, '&amp;')]", rec, tag),
                kind: SuggestionKind::Template,
                category: SuggestionCategory::LinksAndQuality,
            });
        };

        let push_semantic_variants_for_tag = |results: &mut Vec<QuerySuggestion>, tag: &str, rec: &str| {
            if is_pricing_tag(tag) {
                push_pricing_variants_for_tag(results, tag, rec);
            }
            if is_location_tag(tag) {
                push_location_variants_for_tag(results, tag, rec);
            }
            if is_link_tag(tag) {
                push_link_variants_for_tag(results, tag, rec);
            }
            if is_content_tag(tag) {
                push_content_quality_variants_for_tag(results, tag, rec);
            }
        };

        // Determine target active field
        let is_path_mode = input.contains('/') || input.contains('[');
        let active_field = if !term.is_empty() {
            Some(term.as_str())
        } else {
            active_tag.as_deref()
        };

        // 1. High-level topic searches (when user types a keyword in freeform mode)
        let is_pricing_query = !is_path_mode && (term == "bid" || term == "bids" || term == "cpc" || term == "price" || term == "pricing" || term == "budget" || term == "salary" || term == "cost");
        let is_geo_query = !is_path_mode && (term == "geo" || term == "location" || term == "state" || term == "country" || term == "nation" || term == "zip" || term == "us");
        let is_link_query = !is_path_mode && (term == "link" || term == "links" || term == "url" || term == "utm" || term == "http" || term == "https");
        let is_quality_query = !is_path_mode && (term == "qa" || term == "quality" || term == "test" || term == "check");

        if is_empty_query {
            for tag in &self.tags {
                push_empty_variants_for_tag(&mut results, tag, rec_tag);
            }
            for attr in &self.attributes {
                results.push(QuerySuggestion {
                    label: format!("//{}[not(@{})]", rec_tag, attr),
                    description: format!("Find <{}> where @{} attribute is missing", rec_tag, attr),
                    query: format!("//{}[not(@{})]", rec_tag, attr),
                    kind: SuggestionKind::Template,
                    category: SuggestionCategory::EmptyFields,
                });
            }
            let mut seen = HashSet::new();
            results.retain(|s| seen.insert(s.query.clone()));
            results.truncate(60);
            return results;
        }

        if is_pricing_query {
            let pricing_tags: Vec<String> = self.tags.iter().filter(|t| is_pricing_tag(t)).cloned().collect();
            let targets = if !pricing_tags.is_empty() { pricing_tags } else { vec!["cpc".to_string()] };
            for t in &targets {
                push_empty_variants_for_tag(&mut results, t, rec_tag);
                push_pricing_variants_for_tag(&mut results, t, rec_tag);
            }
            let mut seen = HashSet::new();
            results.retain(|s| seen.insert(s.query.clone()));
            results.truncate(60);
            return results;
        }

        if is_geo_query {
            let geo_tags: Vec<String> = self.tags.iter().filter(|t| is_location_tag(t)).cloned().collect();
            let targets = if !geo_tags.is_empty() { geo_tags } else { vec!["country".to_string(), "state".to_string()] };
            for t in &targets {
                push_empty_variants_for_tag(&mut results, t, rec_tag);
                push_location_variants_for_tag(&mut results, t, rec_tag);
            }
            let mut seen = HashSet::new();
            results.retain(|s| seen.insert(s.query.clone()));
            results.truncate(60);
            return results;
        }

        if is_link_query {
            let link_tags: Vec<String> = self.tags.iter().filter(|t| is_link_tag(t)).cloned().collect();
            let targets = if !link_tags.is_empty() { link_tags } else { vec!["url".to_string()] };
            for t in &targets {
                push_empty_variants_for_tag(&mut results, t, rec_tag);
                push_link_variants_for_tag(&mut results, t, rec_tag);
            }
            let mut seen = HashSet::new();
            results.retain(|s| seen.insert(s.query.clone()));
            results.truncate(60);
            return results;
        }

        if is_quality_query {
            let content_tags: Vec<String> = self.tags.iter().filter(|t| is_content_tag(t)).cloned().collect();
            let targets = if !content_tags.is_empty() { content_tags } else { vec!["title".to_string(), "description".to_string()] };
            for t in &targets {
                push_empty_variants_for_tag(&mut results, t, rec_tag);
                push_content_quality_variants_for_tag(&mut results, t, rec_tag);
            }
            let mut seen = HashSet::new();
            results.retain(|s| seen.insert(s.query.clone()));
            results.truncate(60);
            return results;
        }

        // 2. Active field queries prioritized first
        if let Some(target) = active_field {
            if target == rec_tag {
                // If on container/record element (e.g. <job>), suggest matching container + checks for all children
                results.push(QuerySuggestion {
                    label: format!("//{}", rec_tag),
                    description: format!("Match all <{}> records in document", rec_tag),
                    query: format!("//{}", rec_tag),
                    kind: SuggestionKind::Element,
                    category: SuggestionCategory::Elements,
                });
                for tag in &self.tags {
                    if tag != rec_tag && Some(tag) != self.root_tag.as_ref() {
                        push_empty_variants_for_tag(&mut results, tag, rec_tag);
                        push_semantic_variants_for_tag(&mut results, tag, rec_tag);
                        results.push(QuerySuggestion {
                            label: format!("//{}[normalize-space({})!='']", rec_tag, tag),
                            description: format!("Find <{}> where <{}> has valid content", rec_tag, tag),
                            query: format!("//{}[normalize-space({})!='']", rec_tag, tag),
                            kind: SuggestionKind::Template,
                            category: SuggestionCategory::HasData,
                        });
                    }
                }
            } else {
                // Focused on a specific field tag (e.g. <title> or <cpc>)
                push_empty_variants_for_tag(&mut results, target, rec_tag);
                push_semantic_variants_for_tag(&mut results, target, rec_tag);

                if let Some(immediate_parent) = parent_from_path {
                    if immediate_parent != rec_tag && immediate_parent != target {
                        push_empty_variants_for_tag(&mut results, target, immediate_parent);
                        push_semantic_variants_for_tag(&mut results, target, immediate_parent);
                    }
                }
                results.push(QuerySuggestion {
                    label: format!("//{}", target),
                    description: format!("Match all <{}> elements", target),
                    query: format!("//{}", target),
                    kind: SuggestionKind::Element,
                    category: SuggestionCategory::Elements,
                });
                results.push(QuerySuggestion {
                    label: format!("//{}[normalize-space({})!='']", rec_tag, target),
                    description: format!("Find <{}> where <{}> has valid content", rec_tag, target),
                    query: format!("//{}[normalize-space({})!='']", rec_tag, target),
                    kind: SuggestionKind::Template,
                    category: SuggestionCategory::HasData,
                });
                results.push(QuerySuggestion {
                    label: format!("//{}[text()]", target),
                    description: format!("Find <{}> elements that contain text", target),
                    query: format!("//{}[text()]", target),
                    kind: SuggestionKind::Template,
                    category: SuggestionCategory::HasData,
                });
                results.push(QuerySuggestion {
                    label: format!("//{}/{}", rec_tag, target),
                    description: format!("<{}> nested directly in <{}>", target, rec_tag),
                    query: format!("//{}/{}", rec_tag, target),
                    kind: SuggestionKind::Path,
                    category: SuggestionCategory::Elements,
                });

                // If in path browsing mode or input is empty, also append all other feed fields
                if is_path_mode || input.is_empty() {
                    for tag in &self.tags {
                        if tag != rec_tag && tag != target && Some(tag) != self.root_tag.as_ref() {
                            push_empty_variants_for_tag(&mut results, tag, rec_tag);
                            push_semantic_variants_for_tag(&mut results, tag, rec_tag);
                            results.push(QuerySuggestion {
                                label: format!("//{}", tag),
                                description: format!("Match all <{}> elements", tag),
                                query: format!("//{}", tag),
                                kind: SuggestionKind::Element,
                                category: SuggestionCategory::Elements,
                            });
                            results.push(QuerySuggestion {
                                label: format!("//{}[normalize-space({})!='']", rec_tag, tag),
                                description: format!("Find <{}> where <{}> has valid content", rec_tag, tag),
                                query: format!("//{}[normalize-space({})!='']", rec_tag, tag),
                                kind: SuggestionKind::Template,
                                category: SuggestionCategory::HasData,
                            });
                        }
                    }
                }
            }
        }

        // 3. User typed an explicit keyword filter (e.g. "cpc" or "loc" without slashes)
        let mut matched_any_tag = active_field.is_some();
        if !is_path_mode && !term.is_empty() {
            for tag in &self.tags {
                if tag.to_lowercase().contains(&term) && Some(tag.as_str()) != active_field {
                    matched_any_tag = true;
                    if tag != rec_tag {
                        push_empty_variants_for_tag(&mut results, tag, rec_tag);
                        push_semantic_variants_for_tag(&mut results, tag, rec_tag);
                    }
                    results.push(QuerySuggestion {
                        label: format!("//{}", tag),
                        description: format!("Match all <{}> elements", tag),
                        query: format!("//{}", tag),
                        kind: SuggestionKind::Element,
                        category: SuggestionCategory::Elements,
                    });
                    if tag != rec_tag {
                        results.push(QuerySuggestion {
                            label: format!("//{}[normalize-space({})!='']", rec_tag, tag),
                            description: format!("Find <{}> where <{}> has valid content", rec_tag, tag),
                            query: format!("//{}[normalize-space({})!='']", rec_tag, tag),
                            kind: SuggestionKind::Template,
                            category: SuggestionCategory::HasData,
                        });
                    }
                }
            }
        }

        // 4. Fallback: If user typed an arbitrary field name not in sample (e.g. "salary")
        let is_valid_ident = !term.is_empty()
            && term.len() >= 2
            && term.len() <= 32
            && !term.contains('/')
            && !term.contains('[')
            && !term.contains('(')
            && !term.contains(')')
            && !term.contains('=')
            && !term.contains('\'')
            && !term.contains('"')
            && term.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-');

        if !matched_any_tag && is_valid_ident {
            push_empty_variants_for_tag(&mut results, &term, rec_tag);
            push_semantic_variants_for_tag(&mut results, &term, rec_tag);
            results.push(QuerySuggestion {
                label: format!("//{}", term),
                description: format!("Match all <{}> elements", term),
                query: format!("//{}", term),
                kind: SuggestionKind::Element,
                category: SuggestionCategory::Elements,
            });
            results.push(QuerySuggestion {
                label: format!("//{}[normalize-space({})!='']", rec_tag, term),
                description: format!("Find <{}> where <{}> has valid content", rec_tag, term),
                query: format!("//{}[normalize-space({})!='']", rec_tag, term),
                kind: SuggestionKind::Template,
                category: SuggestionCategory::HasData,
            });
            results.push(QuerySuggestion {
                label: format!("//{}[text()]", term),
                description: format!("Find <{}> elements that contain text", term),
                query: format!("//{}[text()]", term),
                kind: SuggestionKind::Template,
                category: SuggestionCategory::HasData,
            });
        }

        // 5. Attribute Suggestions
        for attr in &self.attributes {
            let matches = term.is_empty()
                || attr.to_lowercase().contains(&term)
                || input.starts_with('@')
                || input.contains('@');

            if matches {
                results.push(QuerySuggestion {
                    label: format!("//@{}", attr),
                    description: format!("Match all @{} attributes", attr),
                    query: format!("//@{}", attr),
                    kind: SuggestionKind::Attribute,
                    category: SuggestionCategory::Attributes,
                });

                results.push(QuerySuggestion {
                    label: format!("//{}[not(@{})]", rec_tag, attr),
                    description: format!("Find <{}> where @{} attribute is missing", rec_tag, attr),
                    query: format!("//{}[not(@{})]", rec_tag, attr),
                    kind: SuggestionKind::Template,
                    category: SuggestionCategory::EmptyFields,
                });

                results.push(QuerySuggestion {
                    label: format!("//{}[@{}]", rec_tag, attr),
                    description: format!("Find <{}> with @{} defined", rec_tag, attr),
                    query: format!("//{}[@{}]", rec_tag, attr),
                    kind: SuggestionKind::Template,
                    category: SuggestionCategory::Attributes,
                });
            }
        }

        // Deduplicate suggestions by query string and cap to 80 items
        let mut seen = HashSet::new();
        results.retain(|s| seen.insert(s.query.clone()));
        results.truncate(80);
        results
    }

    fn suggest_json(&self, input: &str) -> Vec<QuerySuggestion> {
        let mut results = Vec::new();
        let query_lower = input.to_lowercase();
        let clean_path = input.trim_start_matches('$');
        let segments: Vec<String> = clean_path
            .split('.')
            .filter(|s| !s.is_empty())
            .map(|s| s.split('[').next().unwrap_or("").trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let leaf = segments.last().map(|s| s.as_str()).unwrap_or("");
        let term = if !leaf.is_empty() && (input.contains('.') || input.contains('[')) {
            leaf.to_lowercase()
        } else {
            query_lower
                .trim_start_matches('$')
                .trim_start_matches('.')
                .trim_start_matches('*')
                .to_string()
        };

        for key in &self.tags {
            if term.is_empty() || key.to_lowercase().contains(&term) {
                // Empty queries
                results.push(QuerySuggestion {
                    label: format!("$[?(!@.{} || @.{} == '')]", key, key),
                    description: format!("Find records where '{}' is missing or empty", key),
                    query: format!("$[?(!@.{} || @.{} == '')]", key, key),
                    kind: SuggestionKind::Template,
                    category: SuggestionCategory::EmptyFields,
                });
                results.push(QuerySuggestion {
                    label: format!("$[?(!@.{})]", key),
                    description: format!("Find records where '{}' property is missing", key),
                    query: format!("$[?(!@.{})]", key),
                    kind: SuggestionKind::Template,
                    category: SuggestionCategory::EmptyFields,
                });
                results.push(QuerySuggestion {
                    label: format!("$[?(@.{} == '')]", key),
                    description: format!("Find records where '{}' is empty string", key),
                    query: format!("$[?(@.{} == '')]", key),
                    kind: SuggestionKind::Template,
                    category: SuggestionCategory::EmptyFields,
                });

                // Has data
                results.push(QuerySuggestion {
                    label: format!("$[?(@.{} != '')]", key),
                    description: format!("Find records where '{}' has data", key),
                    query: format!("$[?(@.{} != '')]", key),
                    kind: SuggestionKind::Template,
                    category: SuggestionCategory::HasData,
                });

                // Pricing templates
                if is_pricing_tag(key) {
                    results.push(QuerySuggestion {
                        label: format!("$[?(@.{} <= 0)]", key),
                        description: format!("Find records where '{}' <= 0 (invalid/zero bid)", key),
                        query: format!("$[?(@.{} <= 0)]", key),
                        kind: SuggestionKind::Template,
                        category: SuggestionCategory::BidsAndPricing,
                    });
                    results.push(QuerySuggestion {
                        label: format!("$[?(@.{} > 2.0)]", key),
                        description: format!("Find high-bid records where '{}' > 2.00", key),
                        query: format!("$[?(@.{} > 2.0)]", key),
                        kind: SuggestionKind::Template,
                        category: SuggestionCategory::BidsAndPricing,
                    });
                }

                // Location templates
                if is_location_tag(key) {
                    results.push(QuerySuggestion {
                        label: format!("$[?(@.{} != 'US')]", key),
                        description: format!("Find international / non-US records ('{}' != 'US')", key),
                        query: format!("$[?(@.{} != 'US')]", key),
                        kind: SuggestionKind::Template,
                        category: SuggestionCategory::LocationAndGeo,
                    });
                    results.push(QuerySuggestion {
                        label: format!("$[?(@.{} == 'US')]", key),
                        description: format!("Find US domestic records ('{}' == 'US')", key),
                        query: format!("$[?(@.{} == 'US')]", key),
                        kind: SuggestionKind::Template,
                        category: SuggestionCategory::LocationAndGeo,
                    });
                }

                // Elements and Paths
                results.push(QuerySuggestion {
                    label: format!("$.{}", key),
                    description: format!("Root property '{}'", key),
                    query: format!("$.{}", key),
                    kind: SuggestionKind::Element,
                    category: SuggestionCategory::Elements,
                });
                results.push(QuerySuggestion {
                    label: format!("$..{}", key),
                    description: format!("Recursive search for '{}' across all objects", key),
                    query: format!("$..{}", key),
                    kind: SuggestionKind::Path,
                    category: SuggestionCategory::Elements,
                });
                results.push(QuerySuggestion {
                    label: format!("$.items[*].{}", key),
                    description: format!("Extract '{}' across items array", key),
                    query: format!("$.items[*].{}", key),
                    kind: SuggestionKind::Template,
                    category: SuggestionCategory::Elements,
                });
            }
        }

        let mut seen = HashSet::new();
        results.retain(|s| seen.insert(s.query.clone()));
        results.truncate(60);
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_field_suggestions_for_explicit_term() {
        let mut disc = DiscoveredTags::default();
        disc.record_tag = Some("job".to_string());
        disc.tags = vec!["job".to_string(), "title".to_string(), "company".to_string()];

        let sugs = disc.suggest("title", None, Some(FileType::Xml));
        assert!(!sugs.is_empty());

        let empty_sugs: Vec<_> = sugs.iter().filter(|s| s.category == SuggestionCategory::EmptyFields).collect();
        assert!(empty_sugs.iter().any(|s| s.query == "//job[not(normalize-space(title))]"));
        assert!(empty_sugs.iter().any(|s| s.query == "//title[not(text())]"));
        assert!(empty_sugs.iter().any(|s| s.query == "//title[normalize-space()='']"));
        assert!(empty_sugs.iter().any(|s| s.query == "//job[not(title)]"));
        assert!(empty_sugs.iter().any(|s| s.query == "//job[title='']"));

        let data_sugs: Vec<_> = sugs.iter().filter(|s| s.category == SuggestionCategory::HasData).collect();
        assert!(data_sugs.iter().any(|s| s.query == "//job[normalize-space(title)!='']"));
    }

    #[test]
    fn test_active_path_prioritizes_empty_queries() {
        let mut disc = DiscoveredTags::default();
        disc.record_tag = Some("job".to_string());
        disc.tags = vec!["job".to_string(), "title".to_string(), "company".to_string()];

        let sugs = disc.suggest("", Some("/jobfeed/jobs/job/title"), Some(FileType::Xml));
        assert!(!sugs.is_empty());
        assert_eq!(sugs[0].query, "//job[not(normalize-space(title))]");
        assert_eq!(sugs[0].category, SuggestionCategory::EmptyFields);
    }

    #[test]
    fn test_empty_keyword_generates_all_empty_field_queries() {
        let mut disc = DiscoveredTags::default();
        disc.record_tag = Some("job".to_string());
        disc.tags = vec!["job".to_string(), "title".to_string(), "city".to_string()];

        let sugs = disc.suggest("empty", None, Some(FileType::Xml));
        assert!(sugs.iter().all(|s| s.category == SuggestionCategory::EmptyFields));
        assert!(sugs.iter().any(|s| s.query == "//job[not(normalize-space(title))]"));
        assert!(sugs.iter().any(|s| s.query == "//job[not(normalize-space(city))]"));
    }

    #[test]
    fn test_discover_xml_prioritizes_container_record_tag() {
        let xml = br#"<jobfeed>
            <job>
                <refcode>REF1</refcode>
                <title>Dev</title>
            </job>
            <job>
                <refcode>REF2</refcode>
                <title>QA</title>
            </job>
        </jobfeed>"#;
        let disc = DiscoveredTags::from_sample(xml, Some(FileType::Xml));
        assert_eq!(disc.record_tag.as_deref(), Some("job"));
    }

    #[test]
    fn test_complex_xpath_input_extracts_leaf_without_runaway_nesting() {
        let xml = br#"<jobfeed>
            <jobs>
                <job>
                    <campaign>
                        <cpc>1.50</cpc>
                    </campaign>
                </job>
            </jobs>
        </jobfeed>"#;
        let disc = DiscoveredTags::from_sample(xml, Some(FileType::Xml));
        let sugs = disc.suggest("//jobfeed/jobs/job/campaign/cpc[normalize-space()='']", None, Some(FileType::Xml));
        assert!(!sugs.is_empty());
        for s in &sugs {
            assert!(s.query.len() < 70, "Query was too long: {}", s.query);
            assert!(!s.query.contains("jobfeed/jobs"), "Nested full path into tag template: {}", s.query);
        }
        assert!(sugs.iter().any(|s| s.query == "//campaign[not(normalize-space(cpc))]"));
        assert!(sugs.iter().any(|s| s.query == "//cpc[not(text())]"));
    }

    #[test]
    fn test_indexed_breadcrumb_path_generates_active_field_and_feed_queries() {
        let mut disc = DiscoveredTags::default();
        disc.record_tag = Some("job".to_string());
        disc.tags = vec!["job".to_string(), "title".to_string(), "company".to_string(), "cpc".to_string()];

        let sugs = disc.suggest("/jobfeed/jobs/job[2]/title", Some("/jobfeed/jobs/job[2]/title"), Some(FileType::Xml));
        assert!(!sugs.is_empty());
        // First suggestion must be for the active field "title"
        assert_eq!(sugs[0].query, "//job[not(normalize-space(title))]");
        assert!(sugs.iter().any(|s| s.query == "//title[not(text())]"));
        assert!(sugs.iter().any(|s| s.query == "//title"));
        assert!(sugs.iter().any(|s| s.query == "//job/title"));

        // Other feed fields should also be available for exploration
        assert!(sugs.iter().any(|s| s.query == "//job[not(normalize-space(company))]"));
        assert!(sugs.iter().any(|s| s.query == "//job[not(normalize-space(cpc))]"));
    }

    #[test]
    fn test_container_record_element_generates_empty_child_queries() {
        let mut disc = DiscoveredTags::default();
        disc.record_tag = Some("job".to_string());
        disc.tags = vec!["job".to_string(), "title".to_string(), "company".to_string()];

        let sugs = disc.suggest("/jobfeed/jobs/job[3]", Some("/jobfeed/jobs/job[3]"), Some(FileType::Xml));
        assert!(!sugs.is_empty());
        assert!(sugs.iter().any(|s| s.query == "//job"));
        assert!(sugs.iter().any(|s| s.query == "//job[not(normalize-space(title))]"));
        assert!(sugs.iter().any(|s| s.query == "//job[not(normalize-space(company))]"));
    }

    #[test]
    fn test_json_indexed_path_generates_key_queries() {
        let mut disc = DiscoveredTags::default();
        disc.tags = vec!["title".to_string(), "author".to_string()];

        let sugs = disc.suggest("$.items[4].title", None, Some(FileType::Json));
        assert!(!sugs.is_empty());
        assert!(sugs.iter().any(|s| s.query == "$[?(!@.title || @.title == '')]"));
        assert!(sugs.iter().any(|s| s.query == "$.title"));
    }

    #[test]
    fn test_pricing_suggestions_for_cpc() {
        let mut disc = DiscoveredTags::default();
        disc.record_tag = Some("job".to_string());
        disc.tags = vec!["job".to_string(), "title".to_string(), "cpc".to_string()];

        let sugs = disc.suggest("cpc", None, Some(FileType::Xml));
        assert!(!sugs.is_empty());

        let pricing_sugs: Vec<_> = sugs.iter().filter(|s| s.category == SuggestionCategory::BidsAndPricing).collect();
        assert!(pricing_sugs.iter().any(|s| s.query == "//job[cpc<=0]"));
        assert!(pricing_sugs.iter().any(|s| s.query == "//job[cpc>2.0]"));
        assert!(pricing_sugs.iter().any(|s| s.query == "//job[cpc<0.50]"));
        assert!(pricing_sugs.iter().any(|s| s.query == "//job[cpc=0]"));
    }

    #[test]
    fn test_location_suggestions_for_country_and_state() {
        let mut disc = DiscoveredTags::default();
        disc.record_tag = Some("job".to_string());
        disc.tags = vec!["job".to_string(), "country".to_string(), "state".to_string()];

        let sugs_country = disc.suggest("country", None, Some(FileType::Xml));
        let geo_country: Vec<_> = sugs_country.iter().filter(|s| s.category == SuggestionCategory::LocationAndGeo).collect();
        assert!(geo_country.iter().any(|s| s.query == "//job[country!='US']"));
        assert!(geo_country.iter().any(|s| s.query == "//job[country='US']"));

        let sugs_state = disc.suggest("state", None, Some(FileType::Xml));
        let geo_state: Vec<_> = sugs_state.iter().filter(|s| s.category == SuggestionCategory::LocationAndGeo).collect();
        assert!(geo_state.iter().any(|s| s.query == "//job[state='CA']"));
        assert!(geo_state.iter().any(|s| s.query == "//job[state='TX']"));
        assert!(geo_state.iter().any(|s| s.query == "//job[string-length(state)!=2]"));
    }

    #[test]
    fn test_link_quality_suggestions_for_url() {
        let mut disc = DiscoveredTags::default();
        disc.record_tag = Some("job".to_string());
        disc.tags = vec!["job".to_string(), "url".to_string()];

        let sugs = disc.suggest("url", None, Some(FileType::Xml));
        let link_sugs: Vec<_> = sugs.iter().filter(|s| s.category == SuggestionCategory::LinksAndQuality).collect();
        assert!(link_sugs.iter().any(|s| s.query == "//job[not(starts-with(url, 'https://'))]"));
        assert!(link_sugs.iter().any(|s| s.query == "//job[not(contains(url, 'utm_source'))]"));
        assert!(link_sugs.iter().any(|s| s.query == "//job[contains(url, 'localhost')]"));
    }
}


