#!/usr/bin/env python3
"""
News scraper: downloads news pages, extracts headings and subtitles, saves report.
"""

import subprocess
import hashlib
import re
import os
import sys
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import urlparse


class HeadingExtractor(HTMLParser):
    """HTML parser to extract headings (h1-h6) and meta descriptions."""
    
    def __init__(self):
        super().__init__()
        self.headings = []  # list of (tag, text)
        self.meta_description = None
        self.og_title = None
        self.og_description = None
        self.title = None
        self._in_title = False
        self._in_heading = None
        self._heading_text = []
        self._current_heading_tag = None
    
    def handle_starttag(self, tag, attrs):
        attrs_dict = dict(attrs)
        
        if tag == 'title':
            self._in_title = True
        
        if tag in ('h1', 'h2', 'h3', 'h4', 'h5', 'h6'):
            self._in_heading = tag
            self._heading_text = []
            self._current_heading_tag = tag
        
        if tag == 'meta':
            name = attrs_dict.get('name', '').lower()
            property_ = attrs_dict.get('property', '').lower()
            content = attrs_dict.get('content', '')
            
            if name == 'description' and content:
                self.meta_description = content.strip()
            elif property_ == 'og:title' and content:
                self.og_title = content.strip()
            elif property_ == 'og:description' and content:
                self.og_description = content.strip()
    
    def handle_endtag(self, tag):
        if tag == 'title':
            self._in_title = False
        
        if tag in ('h1', 'h2', 'h3', 'h4', 'h5', 'h6') and self._in_heading == tag:
            text = ''.join(self._heading_text).strip()
            if text:
                self.headings.append((tag, text))
            self._in_heading = None
            self._current_heading_tag = None
            self._heading_text = []
    
    def handle_data(self, data):
        if self._in_title and self.title is None:
            self.title = data.strip()
        if self._in_heading:
            self._heading_text.append(data)


def get_url_hash(url: str) -> str:
    """Generate a hash for the URL to use as cache filename."""
    # Use SHA256, take first 16 chars for filename
    return hashlib.sha256(url.encode('utf-8')).hexdigest()[:16]


def download_page(url: str, cache_dir: Path) -> tuple[bool, str]:
    """
    Download a page using wget.
    Returns (success, html_content_or_error_message).
    """
    url_hash = get_url_hash(url)
    cache_file = cache_dir / f"{url_hash}.html"
    
    # If already cached, read from cache
    if cache_file.exists():
        try:
            return True, cache_file.read_text(encoding='utf-8', errors='replace')
        except Exception as e:
            return False, f"Cache read error: {e}"
    
    # Download with wget
    try:
        # Use wget with timeout, user agent, and quiet mode
        result = subprocess.run(
            [
                'wget',
                '--quiet',
                '--timeout=15',
                '--tries=2',
                '--user-agent=Mozilla/5.0 (compatible; NewsBot/1.0)',
                '-O', str(cache_file),
                url
            ],
            capture_output=True,
            text=True,
            timeout=30
        )
        
        if result.returncode != 0:
            # Clean up failed download
            if cache_file.exists():
                cache_file.unlink()
            return False, f"wget failed (exit {result.returncode}): {result.stderr.strip()}"
        
        # Read the downloaded content
        html_content = cache_file.read_text(encoding='utf-8', errors='replace')
        return True, html_content
        
    except subprocess.TimeoutExpired:
        if cache_file.exists():
            cache_file.unlink()
        return False, "wget timeout"
    except Exception as e:
        if cache_file.exists():
            cache_file.unlink()
        return False, f"Exception: {e}"


def extract_headings_and_subtitle(html: str, url: str) -> dict:
    """Extract headings and subtitle/description from HTML."""
    parser = HeadingExtractor()
    parser.feed(html)
    
    # Determine main heading and subtitle
    main_heading = None
    subtitle = None
    
    # Priority for main heading: h1 > og:title > <title> > first h2
    if parser.headings:
        for tag, text in parser.headings:
            if tag == 'h1' and not main_heading:
                main_heading = text
                break
        if not main_heading:
            main_heading = parser.headings[0][1]
    
    if not main_heading and parser.og_title:
        main_heading = parser.og_title
    if not main_heading and parser.title:
        main_heading = parser.title
    
    # Priority for subtitle: meta description > og:description > first h2 > first h3
    if parser.meta_description:
        subtitle = parser.meta_description
    elif parser.og_description:
        subtitle = parser.og_description
    else:
        for tag, text in parser.headings:
            if tag in ('h2', 'h3') and not subtitle:
                subtitle = text
                break
    
    return {
        'url': url,
        'main_heading': main_heading or 'No heading found',
        'subtitle': subtitle or 'No subtitle found',
        'all_headings': parser.headings,
        'title': parser.title,
        'og_title': parser.og_title,
        'meta_description': parser.meta_description,
        'og_description': parser.og_description
    }


def read_urls(input_file: Path) -> list[str]:
    """Read URLs from file, strip whitespace, skip empty lines and duplicates."""
    urls = []
    seen = set()
    
    for line in input_file.read_text(encoding='utf-8').splitlines():
        line = line.strip()
        if not line or line.startswith('#'):
            continue
        if line not in seen:
            seen.add(line)
            urls.append(line)
    
    return urls


def generate_report(results: list[dict], output_file: Path):
    """Generate markdown report from extraction results."""
    lines = [
        "# News Reports",
        f"",
        f"Generated from {len(results)} news sources.",
        f"",
        "---",
        ""
    ]
    
    for i, result in enumerate(results, 1):
        url = result['url']
        heading = result['main_heading']
        subtitle = result['subtitle']
        
        lines.extend([
            f"## {i}. {heading}",
            f"",
            f"**Source:** {url}",
            f"",
            f"**Subtitle/Description:** {subtitle}",
            f"",
            "---",
            ""
        ])
    
    output_file.write_text('\n'.join(lines), encoding='utf-8')


def main():
    script_dir = Path(__file__).parent
    data_in_dir = script_dir / "data_in"
    cache_dir = script_dir / "data_cache"
    input_file = data_in_dir / "news-links.txt"
    output_file = script_dir / "news_reports.md"
    
    # Create cache directory
    cache_dir.mkdir(exist_ok=True)
    
    # Read URLs
    if not input_file.exists():
        print(f"Error: {input_file} not found")
        sys.exit(1)
    
    urls = read_urls(input_file)
    print(f"Found {len(urls)} unique URLs to process")
    
    results = []
    
    for i, url in enumerate(urls, 1):
        print(f"[{i}/{len(urls)}] Processing: {url}")
        
        success, content = download_page(url, cache_dir)
        
        if success:
            result = extract_headings_and_subtitle(content, url)
            results.append(result)
            print(f"  ✓ Heading: {result['main_heading'][:80]}...")
            if result['subtitle']:
                print(f"  ✓ Subtitle: {result['subtitle'][:80]}...")
        else:
            print(f"  ✗ Failed: {content}")
            results.append({
                'url': url,
                'main_heading': 'DOWNLOAD FAILED',
                'subtitle': content,
                'all_headings': [],
                'title': None,
                'og_title': None,
                'meta_description': None,
                'og_description': None
            })
    
    # Generate report
    generate_report(results, output_file)
    print(f"\nReport saved to: {output_file}")


if __name__ == "__main__":
    main()
