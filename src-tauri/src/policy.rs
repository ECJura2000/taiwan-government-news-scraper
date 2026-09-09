//! Versioned, portable policy configuration shared by the CLI and desktop UI.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    io::{Read, Write},
    path::Path,
};
use unicode_normalization::UnicodeNormalization;

pub const MAX_POLICY_BYTES: u64 = 2 * 1024 * 1024;
fn yes() -> bool {
    true
}
fn version() -> u32 {
    2
}
fn fields() -> Vec<String> {
    vec!["title".into(), "summary".into()]
}
fn penalty() -> u32 {
    50
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Scoring {
    pub k1: f64,
    pub b: f64,
    pub title_weight: f64,
    pub summary_weight: f64,
    pub general_weight: f64,
}
impl Default for Scoring {
    fn default() -> Self {
        Self {
            k1: 1.2,
            b: 0.75,
            title_weight: 3.0,
            summary_weight: 1.0,
            general_weight: 0.5,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Thresholds {
    pub high: u32,
    pub possible: u32,
    pub negative_penalty: u32,
}
impl Default for Thresholds {
    fn default() -> Self {
        Self {
            high: 80,
            possible: 40,
            negative_penalty: 50,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Keyword {
    pub text: String,
    pub weight: f64,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub origin: String,
    #[serde(default)]
    pub references: Vec<Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NegativeRule {
    pub text: String,
    #[serde(default = "penalty")]
    pub penalty: u32,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default = "fields")]
    pub match_fields: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Initiative {
    pub name: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub lead_agency: String,
    #[serde(default)]
    pub lead_source: String,
    #[serde(default)]
    pub exact_phrases: Vec<String>,
    #[serde(default)]
    pub strong_keywords: Vec<String>,
    #[serde(default)]
    pub context_keywords: Vec<String>,
    #[serde(default)]
    pub weighted_keywords: Vec<Keyword>,
    #[serde(default)]
    pub penalty_keywords: Vec<NegativeRule>,
    #[serde(default)]
    pub exclude_keywords: Vec<NegativeRule>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    #[serde(default = "version")]
    pub schema_version: u32,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub name: String,
    pub initiatives: Vec<Initiative>,
    #[serde(default)]
    pub general_keywords: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub negative_keywords: Vec<String>,
    #[serde(default)]
    pub thresholds: Thresholds,
    #[serde(default)]
    pub scoring: Scoring,
    #[serde(default)]
    pub references: Vec<Value>,
}

pub fn normalized_name(text: &str) -> String {
    text.nfkc()
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
impl Profile {
    pub fn embedded() -> Self {
        Self::parse(include_str!("../resources/relevance-policy.json")).expect("內建主題設定應有效")
    }
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.len() as u64 > MAX_POLICY_BYTES {
            return Err("主題 JSON 不得超過 2 MiB".into());
        }
        let mut profile: Self = serde_json::from_str(text.trim_start_matches('\u{feff}'))
            .map_err(|e| format!("主題 JSON 格式錯誤：{e}"))?;
        if !profile.negative_keywords.is_empty() {
            for topic in &mut profile.initiatives {
                for word in &profile.negative_keywords {
                    topic.penalty_keywords.push(NegativeRule {
                        text: word.clone(),
                        penalty: profile.thresholds.negative_penalty,
                        enabled: true,
                        match_fields: vec!["title".into()],
                    });
                }
            }
            profile.negative_keywords.clear();
        }
        profile.validate()?;
        profile.schema_version = 2;
        Ok(profile)
    }
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=2).contains(&self.schema_version) {
            return Err("不支援此主題設定格式版本".into());
        }
        if self.initiatives.len() > 100 {
            return Err("主題數不得超過 100".into());
        }
        if self.thresholds.possible == 0
            || self.thresholds.possible >= self.thresholds.high
            || self.thresholds.high > 100
            || self.thresholds.negative_penalty > 100
        {
            return Err("門檻須符合 0 < 可能相關 < 高度相關 ≤ 100；扣分不得超過 100".into());
        }
        let s = &self.scoring;
        if !s.b.is_finite()
            || !(0.0..=1.0).contains(&s.b)
            || [s.k1, s.title_weight, s.summary_weight, s.general_weight]
                .iter()
                .any(|v| !v.is_finite() || *v <= 0.0 || *v > 100.0)
        {
            return Err("BM25 權重及 k1 須介於 0 與 100，b 須介於 0 與 1".into());
        }
        for text in &self.general_keywords {
            valid_text(text, "共用一般詞")?;
        }
        let mut names = BTreeSet::new();
        for t in &self.initiatives {
            if normalized_name(&t.name).is_empty()
                || t.name.chars().count() > 120
                || !names.insert(normalized_name(&t.name))
            {
                return Err(format!("主題名稱空白、過長或重複：{}", t.name));
            }
            for text in t
                .strong_keywords
                .iter()
                .chain(&t.context_keywords)
                .chain(&t.exact_phrases)
            {
                valid_text(text, &t.name)?;
            }
            for k in &t.weighted_keywords {
                valid_text(&k.text, &t.name)?;
                if !k.weight.is_finite() || k.weight <= 0.0 || k.weight > 100.0 {
                    return Err(format!(
                        "{}：{} 的權重須大於 0 且不超過 100",
                        t.name, k.text
                    ));
                }
            }
            for r in t.penalty_keywords.iter().chain(&t.exclude_keywords) {
                valid_text(&r.text, &t.name)?;
                if r.penalty > 100
                    || r.match_fields.is_empty()
                    || r.match_fields
                        .iter()
                        .any(|f| f != "title" && f != "summary")
                {
                    return Err(format!("{}：{} 的扣分或比對欄位不正確", t.name, r.text));
                }
            }
        }
        Ok(())
    }
    pub fn require_enabled(&self) -> Result<(), String> {
        self.validate()?;
        if self.initiatives.iter().any(|t| t.enabled) {
            Ok(())
        } else {
            Err("請至少啟用一個搜尋主題".into())
        }
    }
    pub fn hash(&self) -> String {
        let mut value = serde_json::to_value(self).expect("validated policy serializes");
        value.as_object_mut().unwrap().remove("name");
        value["tokenizer"] = json!("jieba-rs 0.10.3");
        value["dictionary"] = json!(include_str!("../resources/policy-dictionary.txt"));
        let hash = Sha256::digest(serde_json::to_vec(&value).unwrap());
        hash.iter().map(|b| format!("{b:02x}")).collect()
    }
    pub fn summary(&self) -> Value {
        let keywords: Vec<_> = self
            .initiatives
            .iter()
            .flat_map(|t| {
                t.weighted_keywords
                    .iter()
                    .map(move |k| t.enabled && k.enabled)
            })
            .collect();
        let legacy = self
            .initiatives
            .iter()
            .map(|t| t.strong_keywords.len() + t.context_keywords.len() + t.exact_phrases.len())
            .sum::<usize>();
        let enabled_legacy = self
            .initiatives
            .iter()
            .filter(|t| t.enabled)
            .map(|t| t.strong_keywords.len() + t.context_keywords.len() + t.exact_phrases.len())
            .sum::<usize>();
        let count = keywords.len() + legacy + self.general_keywords.len();
        let enabled =
            keywords.iter().filter(|&&v| v).count() + enabled_legacy + self.general_keywords.len();
        let penalties = self
            .initiatives
            .iter()
            .map(|t| t.penalty_keywords.len())
            .sum::<usize>();
        let exclusions = self
            .initiatives
            .iter()
            .map(|t| t.exclude_keywords.len())
            .sum::<usize>();
        let enabled_penalties = self
            .initiatives
            .iter()
            .filter(|t| t.enabled)
            .flat_map(|t| &t.penalty_keywords)
            .filter(|r| r.enabled)
            .count();
        let enabled_exclusions = self
            .initiatives
            .iter()
            .filter(|t| t.enabled)
            .flat_map(|t| &t.exclude_keywords)
            .filter(|r| r.enabled)
            .count();
        json!({"name":self.name,"schema_version":2,"template_version":self.version,"ruleset_hash":self.hash(),"source":"本次主題設定","topic_count":self.initiatives.len(),"enabled_topic_count":self.initiatives.iter().filter(|t|t.enabled).count(),"disabled_topic_count":self.initiatives.iter().filter(|t|!t.enabled).count(),"keyword_count":count,"enabled_keyword_count":enabled,"disabled_keyword_count":count-enabled,"penalty_count":penalties,"enabled_penalty_count":enabled_penalties,"exclusion_count":exclusions,"enabled_exclusion_count":enabled_exclusions,"disabled_exclusion_count":exclusions-enabled_exclusions,"tokenizer":"jieba-rs 0.10.3","dictionary_version":"zh-Hant-policy-1","scoring":self.scoring,"references":self.references,"effective_policy":self})
    }
}
fn valid_text(text: &str, topic: &str) -> Result<(), String> {
    if text.trim().is_empty() || text.chars().count() > 300 {
        Err(format!("{topic}：關鍵詞不得空白或超過 300 字"))
    } else {
        Ok(())
    }
}
pub fn read_profile(path: &Path) -> Result<Profile, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("無法讀取主題設定：{e}"))?;
    let mut text = String::new();
    file.take(MAX_POLICY_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|e| format!("無法讀取 UTF-8 主題設定：{e}"))?;
    Profile::parse(&text)
}
pub fn save_profile(path: &Path, profile: &Profile) -> Result<(), String> {
    profile.validate()?;
    let bytes = serde_json::to_vec_pretty(profile).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_POLICY_BYTES {
        return Err("主題 JSON 不得超過 2 MiB".into());
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    temp.write_all(&bytes).map_err(|e| e.to_string())?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    temp.persist(path)
        .map_err(|e| format!("無法儲存設定，原設定保留：{e}"))?;
    Ok(())
}
pub fn preview_import(current: &Profile, text: &str, replace: bool) -> Result<Value, String> {
    let incoming = Profile::parse(text)?;
    let raw: Value =
        serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())?;
    let mut result = if replace {
        incoming.clone()
    } else {
        current.clone()
    };
    let mut added = Vec::new();
    let mut updated = Vec::new();
    let mut deleted = Vec::new();
    for t in &incoming.initiatives {
        let old = current
            .initiatives
            .iter()
            .any(|old| normalized_name(&old.name) == normalized_name(&t.name));
        if old {
            updated.push(t.name.clone());
        } else {
            added.push(t.name.clone());
        }
        if !replace {
            if let Some(i) = result
                .initiatives
                .iter()
                .position(|old| normalized_name(&old.name) == normalized_name(&t.name))
            {
                result.initiatives[i] = t.clone();
            } else {
                result.initiatives.push(t.clone());
            }
        }
    }
    if replace {
        for old in &current.initiatives {
            if !incoming
                .initiatives
                .iter()
                .any(|t| normalized_name(&t.name) == normalized_name(&old.name))
            {
                deleted.push(old.name.clone());
            }
        }
    }
    let mut common_changes = Vec::new();
    let before = serde_json::to_value(current).unwrap();
    let mut after = serde_json::to_value(&result).unwrap();
    let supplied = serde_json::to_value(&incoming).unwrap();
    for key in [
        "scoring",
        "thresholds",
        "general_keywords",
        "references",
        "name",
        "version",
    ] {
        if raw.get(key).is_some() {
            // A supplied nested setting replaces that section; preview displays its effective defaults too.
            after[key] = supplied[key].clone();
        }
        if after[key] != before[key] {
            common_changes.push(json!({"field":key,"before":before[key],"after":after[key]}));
        }
    }
    result = Profile::parse(&serde_json::to_string(&after).unwrap())?;
    Ok(
        json!({"profile":result,"added":added,"updated":updated,"deleted":deleted,"common_changes":common_changes}),
    )
}
