# Web API

The Web API provides methods for web scraping and browser automation.

## Overview

Web scraping capabilities allow you to extract content from web pages, take screenshots, and automate browser interactions.

## Basic Usage

```python
from dsb_sdk import DSBClient

client = DSBClient()

# Scrape a webpage
result = client.web.scrape(
    url="https://example.com",
    format="markdown",  # or "html", "text"
)
print(result.content)
```

## Web Scraping

### Scrape as Markdown

```python
result = client.web.scrape(
    url="https://example.com",
    format="markdown",
)
print(result.content)
```

### Scrape as HTML

```python
result = client.web.scrape(
    url="https://example.com",
    format="html",
)
print(result.html)
```

### Extract Tables

```python
tables = client.web.extract_tables(
    url="https://example.com",
)
for table in tables:
    print(f"Headers: {table.headers}")
    print(f"Rows: {table.rows}")
```

### Extract CSS Selectors

```python
elements = client.web.extract_css(
    url="https://example.com",
    selector=".article-content p",
)
for element in elements:
    print(element.text)
```

## Web Search

### Google Search

```python
results = client.web.search(
    query="Python SDK examples",
    engine="google",
    num_results=10,
)
for result in results:
    print(f"Title: {result.title}")
    print(f"URL: {result.url}")
```

### DuckDuckGo Search

```python
results = client.web.search(
    query="Docker containers",
    engine="duckduckgo",
)
```

### Bing Search

```python
results = client.web.search(
    query="Web scraping tutorial",
    engine="bing",
)
```

## Get Links

```python
links = client.web.links(url="https://example.com")
for link in links:
    print(f"Text: {link.text}")
    print(f"URL: {link.url}")
```

## Crawl Website

```python
crawl = client.web.crawl(
    url="https://example.com",
    max_pages=10,
    follow_links=True,
)
for page in crawl.pages:
    print(f"URL: {page.url}")
    print(f"Title: {page.title}")
```

## Screenshot

```python
screenshot = client.web.screenshot(
    url="https://example.com",
    format="png",  # or "jpeg", "webp"
    width=1920,
    height=1080,
)
with open("screenshot.png", "wb") as f:
    f.write(screenshot)
```

## Browser Automation

### Navigate

```python
client.web.browser_navigate(
    url="https://example.com",
)
```

### Click Elements

```python
# Click by index
client.web.browser_click(
    index=0,  # First clickable element
)

# Click by CSS selector
client.web.browser_click(
    selector="#submit-button",
)
```

### Fill Forms

```python
client.web.browser_fill(
    selector="input[name='email']",
    value="test@example.com",
)
```

### Scroll

```python
# Scroll down
client.web.browser_scroll(direction="down")

# Scroll up
client.web.browser_scroll(direction="up")

# Scroll to bottom
client.web.browser_scroll(direction="bottom")
```

### Take Screenshot

```python
client.web.browser_screenshot(
    path="/tmp/screenshot.png",
)
```

### Execute JavaScript

```python
result = client.web.browser_evaluate(
    script="document.title",
)
print(f"Page title: {result}")
```

### Multiple Tabs

```python
# Open new tab
client.web.browser_new_tab(url="https://example.com")

# List tabs
tabs = client.web.browser_tab_list()

# Switch tab
client.web.browser_switch_tab(index=1)

# Close tab
client.web.browser_close()
```

### Go Back/Forward

```python
client.web.browser_go_back()
client.web.browser_go_forward()
```

## Async Usage

```python
import asyncio
from dsb_sdk import AsyncDSBClient

async def main():
    async with AsyncDSBClient() as client:
        result = await client.web.scrape_async(
            url="https://example.com",
            format="markdown",
        )
        print(result.content)

asyncio.run(main())
```

## API Reference

### Scraping Methods

| Method | Description |
|--------|-------------|
| `scrape(url, format)` | Scrape webpage content |
| `extract_tables(url)` | Extract tables from page |
| `extract_css(url, selector)` | Extract elements by CSS |

### Search Methods

| Method | Description |
|--------|-------------|
| `search(query, engine, num_results)` | Search web |
| `links(url)` | Get all links from page |

### Crawl Methods

| Method | Description |
|--------|-------------|
| `crawl(url, max_pages, follow_links)` | Crawl website |

### Screenshot Methods

| Method | Description |
|--------|-------------|
| `screenshot(url, format, width, height)` | Take screenshot |

### Browser Automation

| Method | Description |
|--------|-------------|
| `browser_navigate(url)` | Navigate to URL |
| `browser_click(index, selector)` | Click element |
| `browser_fill(selector, value)` | Fill form field |
| `browser_scroll(direction)` | Scroll page |
| `browser_screenshot(path)` | Take browser screenshot |
| `browser_evaluate(script)` | Execute JavaScript |
| `browser_new_tab(url)` | Open new tab |
| `browser_tab_list()` | List open tabs |
| `browser_switch_tab(index)` | Switch to tab |
| `browser_close()` | Close current tab, switch to adjacent (or clear if last tab) |
| `browser_go_back()` | Go back in history |
| `browser_go_forward()` | Go forward in history |

### Async Methods

All methods have async equivalents with `_async` suffix.
