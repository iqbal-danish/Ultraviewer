#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PythonTemplate {
    pub id: &'static str,
    pub title: &'static str,
    pub icon: &'static str,
    pub description: &'static str,
    pub code: &'static str,
}

pub const ALL_TEMPLATES: &[PythonTemplate] = &[
    PythonTemplate {
        id: "xml_tag_extractor",
        title: "XML Tag Extractor",
        icon: "[XML]",
        description: "Extract unique values of any XML tag (<job_id>, <title>, etc.) with O(1) streaming regex.",
        code: r#""""
UltraViewer XML Tag Extractor
Extracts unique occurrences of any XML tag directly from the active file.
"""
import os
import re

# ==========================================
# CONFIGURATION
# ==========================================
TARGET_TAG = "job_id"   # Change to: title, company, url, category, etc.
PREVIEW_LIMIT = 50      # Number of sample values to print in console

file_path = os.environ.get("ULTRAVIEWER_FILE_PATH", "")
if not file_path:
    print("❌ No active file detected. Please open an XML file in another tab.")
    exit(1)

print(f"📂 Scanning XML: {os.path.basename(file_path)}")
print(f"🎯 Target Tag:   <{TARGET_TAG}>...</{TARGET_TAG}>\n")

pattern = re.compile(rf"<{TARGET_TAG}>(?:<!\[CDATA\[)?(.*?)(?:\]\]>)?</{TARGET_TAG}>", re.IGNORECASE | re.DOTALL)

unique_values = set()
total_matches = 0

with open(file_path, "r", encoding="utf-8", errors="replace") as f:
    for line in f:
        matches = pattern.findall(line)
        for m in matches:
            total_matches += 1
            unique_values.add(m.strip())

print(f"✅ Extraction Complete:")
print(f"   • Total occurrences found: {total_matches:,}")
print(f"   • Unique values count:     {len(unique_values):,}\n")
print(f"--- Sample Preview (First {min(len(unique_values), PREVIEW_LIMIT)}) ---")
for val in sorted(list(unique_values))[:PREVIEW_LIMIT]:
    print(val)
"#,
    },
    PythonTemplate {
        id: "json_streaming_extractor",
        title: "JSON / JSONL Key Extractor",
        icon: "[JSON]",
        description: "Parse large JSON/JSONL records line-by-line without memory limits and extract target fields.",
        code: r#""""
UltraViewer JSON / JSONL Streaming Key Extractor
Extracts specific fields from JSON records line-by-line without high RAM usage.
"""
import os
import json

# ==========================================
# CONFIGURATION
# ==========================================
TARGET_KEY = "title"     # Change to: id, name, company, salary, location, etc.
OUTPUT_FILE = ""         # e.g. "extracted_values.txt" (leave empty to print only)
PREVIEW_LIMIT = 50

file_path = os.environ.get("ULTRAVIEWER_FILE_PATH", "")
if not file_path:
    print("❌ No active file detected. Please open a JSON/JSONL file in another tab.")
    exit(1)

print(f"📂 Streaming JSON: {os.path.basename(file_path)}")
print(f"🎯 Target Key:     '{TARGET_KEY}'\n")

unique_values = set()
total_records = 0

with open(file_path, "r", encoding="utf-8", errors="replace") as f:
    for line in f:
        line = line.strip().rstrip(",")
        if not line or line in ("[", "]"):
            continue
        try:
            record = json.loads(line)
            if isinstance(record, dict) and TARGET_KEY in record:
                val = str(record[TARGET_KEY]).strip()
                if val:
                    unique_values.add(val)
                    total_records += 1
        except Exception:
            continue

print(f"✅ Processed {total_records:,} matching records.")
print(f"   • Unique '{TARGET_KEY}' count: {len(unique_values):,}\n")
print(f"--- Sample Preview (First {min(len(unique_values), PREVIEW_LIMIT)}) ---")
for val in list(unique_values)[:PREVIEW_LIMIT]:
    print(val)

if OUTPUT_FILE:
    with open(OUTPUT_FILE, "w", encoding="utf-8") as out:
        for val in sorted(unique_values):
            out.write(val + "\n")
    print(f"\n💾 Saved {len(unique_values):,} values to: {OUTPUT_FILE}")
"#,
    },
    PythonTemplate {
        id: "csv_column_filter",
        title: "CSV Column Filter",
        icon: "[CSV]",
        description: "Filter and aggregate rows by column criteria from the active CSV/TSV table.",
        code: r#""""
UltraViewer CSV Fast Column Filter
Filters rows by column values and aggregates distinct statistics.
"""
import os
import csv

# ==========================================
# CONFIGURATION
# ==========================================
FILTER_COLUMN = "country"   # Column header to inspect
FILTER_VALUE = "US"         # Value to match (case-insensitive substring)
TOP_N = 25

file_path = os.environ.get("ULTRAVIEWER_FILE_PATH", "")
if not file_path:
    print("❌ No active file detected. Please open a CSV file in another tab.")
    exit(1)

print(f"📂 Scanning CSV: {os.path.basename(file_path)}")
print(f"🔍 Filter:       {FILTER_COLUMN} == '{FILTER_VALUE}'\n")

matched_rows = 0
total_rows = 0

with open(file_path, "r", encoding="utf-8", errors="replace") as f:
    reader = csv.DictReader(f)
    print(f"📋 Headers detected: {', '.join(reader.fieldnames or [])}\n")
    
    for row in reader:
        total_rows += 1
        col_val = row.get(FILTER_COLUMN, "")
        if FILTER_VALUE.lower() in col_val.lower():
            matched_rows += 1
            if matched_rows <= TOP_N:
                print(f"Row {total_rows}: {dict(row)}")

print(f"\n✅ Total Rows Processed: {total_rows:,}")
print(f"🎯 Matching Rows:        {matched_rows:,}")
"#,
    },
    PythonTemplate {
        id: "async_api_crawler",
        title: "Async API Ingestion Crawler",
        icon: "[API]",
        description: "Stream paginated API calls with retry backoff directly into a local JSONL file.",
        code: r#""""
UltraViewer Async API Ingestion Crawler
Fetches paginated JSON batches with retry backoff and streams to JSONL disk.
"""
import asyncio
import httpx
import json
import os

# ==========================================
# CONFIGURATION
# ==========================================
BASE_URL = "https://api.wisestep.com/career-page/jobs"
HEADERS = {
    "user-agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/124.0.0.0 Safari/537.36",
    "accept": "application/json",
}
PAGE_SIZE = 20
OUTPUT_FILE = "jobs_catalog.jsonl"
MAX_PAGES = 10  # Set to None to fetch everything

async def fetch_page(client, page: int):
    payload = {"page": page, "size": PAGE_SIZE}
    resp = await client.post(BASE_URL, headers=HEADERS, json=payload, timeout=20.0)
    return resp.json() if resp.status_code == 200 else None

async def run_crawler():
    print(f"🚀 Crawler started -> Target Output: {OUTPUT_FILE}")
    total_ingested = 0
    page = 0

    async with httpx.AsyncClient() as client:
        with open(OUTPUT_FILE, "a", encoding="utf-8") as f_out:
            while MAX_PAGES is None or page < MAX_PAGES:
                data = await fetch_page(client, page)
                if not data:
                    break
                items = data if isinstance(data, list) else (
                    data.get("jobs") or data.get("data") or data.get("items") or []
                )
                if not items:
                    break
                for item in items:
                    f_out.write(json.dumps(item, ensure_ascii=False) + "\n")
                total_ingested += len(items)
                print(f"📦 Page {page}: Ingested {len(items)} items (Progress: {total_ingested})")
                page += 1
                await asyncio.sleep(0.3)

    print(f"🎉 Ingestion finished! {total_ingested} records written to {OUTPUT_FILE}")

if __name__ == "__main__":
    asyncio.run(run_crawler())
"#,
    },
    PythonTemplate {
        id: "xml_cdata_cleaner",
        title: "XML CDATA HTML Cleaner",
        icon: "[HTML]",
        description: "Strip nested HTML tags and unescape entities from XML job descriptions/summaries.",
        code: r#""""
UltraViewer XML CDATA Description Cleaner
Extracts job descriptions, strips raw HTML tags and entities into clean text.
"""
import os
import re
import html

# ==========================================
# CONFIGURATION
# ==========================================
TARGET_TAG = "description"  # or summary, content, details
OUTPUT_FILE = "cleaned_descriptions.txt"
PREVIEW_LIMIT = 5

file_path = os.environ.get("ULTRAVIEWER_FILE_PATH", "")
if not file_path:
    print("❌ No active file detected. Please open an XML file in another tab.")
    exit(1)

print(f"📂 Cleaning XML descriptions from: {os.path.basename(file_path)}")

tag_regex = re.compile(rf"<{TARGET_TAG}>(?:<!\[CDATA\[)?(.*?)(?:\]\]>)?</{TARGET_TAG}>", re.IGNORECASE | re.DOTALL)
html_cleaner = re.compile(r"<[^>]+>")

cleaned_count = 0
with open(file_path, "r", encoding="utf-8", errors="replace") as f_in, \
     open(OUTPUT_FILE, "w", encoding="utf-8") as f_out:
    
    content = f_in.read()
    for raw in tag_regex.findall(content):
        # Unescape HTML entities & strip tags
        clean_text = html.unescape(html_cleaner.sub(" ", raw))
        clean_text = " ".join(clean_text.split())
        f_out.write(clean_text + "\n---\n")
        cleaned_count += 1
        if cleaned_count <= PREVIEW_LIMIT:
            print(f"Sample {cleaned_count}:\n{clean_text[:200]}...\n")

print(f"✅ Processed {cleaned_count:,} descriptions saved to {OUTPUT_FILE}")
"#,
    },
];

pub fn get_template_by_id(id: &str) -> Option<&'static PythonTemplate> {
    ALL_TEMPLATES.iter().find(|t| t.id == id)
}
