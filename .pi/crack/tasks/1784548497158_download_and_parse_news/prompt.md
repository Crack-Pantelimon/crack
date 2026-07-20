at _data/news/data_in/news-links.txt we have put some links with news sites.

In the main file here _data/news/data_in/news-links.txt you should:
- strip the line of whitespace and skip any duplicates
- for each link, download its page using wget into ./data_cache/<link_hash>.html
- use subprocess so we don't pull any dependencies ,. but regardless only run the main script by doing "cd _data/news/ && uv run main.py" 
- look at the html and extract a sample news heading and subtitle from each file and save the report in a file _data/news/news_reports.md 

You should only ever work on the _data/news folder , don't read or edit anything else in the repo please. 