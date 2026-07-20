

## Auto-generated signatures
<!-- Updated by gen-context.js -->
# Code signatures

## SigMap commands

| When | Command |
|------|---------|
| Before answering a question about code | `sigmap ask "<your question>"` |
| To rank files by topic | `sigmap --query "<topic>"` |
| After changing config or source dirs | `sigmap validate` |
| To verify an AI answer is grounded | `sigmap judge --response <file>` |

Always run `sigmap ask` (or `sigmap --query`) before searching for files relevant to a task.

## .

### main.py
```
class HeadingExtractor(HTMLParser)  :16-70
  def __init__()
  def handle_starttag(tag, attrs)
  def handle_endtag(tag)
  def handle_data(data)
def get_url_hash(url: str) → str  :73-76  # Generate a hash for the URL to use as cache filename
def download_page(url: str, cache_dir: Path) → tuple[bool, str]  :79-129  # Download a page using wget
def extract_headings_and_subtitle(html: str, url: str) → dict  :132-175  # Extract headings and subtitle/description from HTML
def read_urls(input_file: Path) → list[str]  :178-191  # Read URLs from file, strip whitespace, skip empty lines and 
def generate_report(results: list[dict], output_file: Path)  :194-221  # Generate markdown report from extraction results
def main()  :224-270
```

### news_reports.md
```
h1 News Reports
h2 1. Tendințe
h2 2. Tendințe
h2 3. Tendințe
h2 4. Tendințe
h2 5. Tendințe
h2 6. Tendințe
h2 7. Tendințe
h2 8. Tendințe
h2 9. Tendințe
h2 10. Tendințe
h2 11. Tendințe
h2 12. Tendințe
h2 13. Tendințe
h2 14. Articole
h2 15. 7lucruri
h2 16. 7lucruri
h2 17. Politic
h2 18. Monden
h2 19. Social
h2 20. IT & Știința
h2 21. Descoperă România
h2 22. Blogul TimesNewRoman
h2 23. Rezultatele cautarii dupa: "cacat"
h2 24. Rezultatele cautarii dupa: "pisat"
h2 25. Rezultatele cautarii dupa: "sabie"
h2 26. Rezultatele cautarii dupa: "cutit"
h2 27. Rezultatele cautarii dupa: "boschetar"
```

### pyproject.toml
```
table [project]
key name
key version
key description
key readme
key requires-python
key dependencies
```
