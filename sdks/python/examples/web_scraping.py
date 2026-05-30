"""
Web scraping example using DSB SDK

This example demonstrates how to use the web scraping and browser automation
features of the DSB SDK.

Requirements:
- A running DSB server
- A sandbox with web tools (use 'dsb/sandbox' image, not 'dsb/sandbox-slim')
"""

import time

from dsb_sdk import DSBClient


def main():
    """Main function demonstrating web scraping features."""
    client = DSBClient(api_url="http://localhost:8080")

    try:
        # Create a sandbox with web tools
        print("Creating sandbox...")
        sandbox = client.sandbox.create(
            image="dsb/sandbox:latest",
            name="web-scraping-demo",
        )
        print(f"Sandbox created: {sandbox.id}")

        # Wait for sandbox to be ready
        print("Waiting for sandbox to be ready...")
        max_wait = 60
        for i in range(max_wait):
            time.sleep(1)
            sandbox = client.sandbox.get(sandbox.id)
            if sandbox.state.value == "running":
                print(f"Sandbox is running after {i + 1} seconds")
                break
        else:
            print("Timeout waiting for sandbox to start")
            return

        # Give web tools a moment to initialize
        time.sleep(3)

        # =====================================================================
        # Web Scraping
        # =====================================================================
        print("\n" + "=" * 60)
        print("WEB SCRAPING")
        print("=" * 60)

        # Basic scraping
        print("\n1. Scraping https://example.com as markdown...")
        result = client.web.scrape(sandbox.id, "https://example.com", format="markdown")
        print(f"   Title: {result.title}")
        print(f"   Content preview: {result.content[:200]}...")

        # Scraping with screenshot
        print("\n2. Scraping with screenshot...")
        result = client.web.scrape(
            sandbox.id, "https://example.com", screenshot=True, format="markdown"
        )
        print(f"   Screenshot path: {result.screenshot_path}")

        # =====================================================================
        # Advanced Content Filtering (NEW)
        # =====================================================================
        print("\n" + "=" * 60)
        print("ADVANCED CONTENT FILTERING")
        print("=" * 60)

        # BM25 content filtering for query-relevant content
        print("\n3. Using BM25 filtering for query-relevant content...")
        result = client.web.scrape(
            sandbox.id,
            "https://example.com",
            format="markdown",
            search_query="example domain information",
            bm25_threshold=1.0,
        )
        print(f"   Title: {result.title}")
        print(f"   Filtered content preview: {result.content[:200]}...")

        # Pruning filter to remove low-quality content
        print("\n4. Using pruning filter to remove low-quality content...")
        result = client.web.scrape(
            sandbox.id,
            "https://example.com",
            format="markdown",
            use_pruning=True,
            pruning_threshold=0.48,
        )
        print(f"   Pruned content preview: {result.content[:200]}...")

        # Max length truncation
        print("\n5. Limiting content length with max_length...")
        result = client.web.scrape(
            sandbox.id,
            "https://example.com",
            format="markdown",
            max_length=500,
        )
        print(f"   Truncated content length: {len(result.content)} characters")
        print(f"   Content: {result.content[:200]}...")

        # =====================================================================
        # Web Search
        # =====================================================================
        print("\n" + "=" * 60)
        print("WEB SEARCH")
        print("=" * 60)

        print("\n6. Searching for 'Python async await tutorial' on Google...")
        search_results = client.web.search(
            sandbox.id, "Python async await tutorial", engine="google", num_results=5
        )
        print(f"   Query: {search_results.query}")
        print(f"   Engine: {search_results.engine}")
        print(f"   Results: {len(search_results.results)}")
        for i, r in enumerate(search_results.results[:3], 1):
            print(f"   {i}. {r.title}")
            print(f"      URL: {r.url}")
            print(f"      Snippet: {r.snippet[:100]}...")

        # =====================================================================
        # Link Extraction
        # =====================================================================
        print("\n" + "=" * 60)
        print("LINK EXTRACTION")
        print("=" * 60)

        print("\n7. Extracting links from example.com...")
        links = client.web.links(sandbox.id, "https://example.com")
        print(f"   Total links: {links.total_links}")
        print("   First 5 links:")
        for link in links.links[:5]:
            print(f"      - {link}")

        # =====================================================================
        # Browser Automation
        # =====================================================================
        print("\n" + "=" * 60)
        print("BROWSER AUTOMATION")
        print("=" * 60)

        # Check browser support first
        print("\n8. Checking browser support...")
        if client.web.supports_browser(sandbox.id):
            print("   Browser automation is supported!")

            # Navigate
            print("\n9. Navigating to httpbin.org/html...")
            nav = client.web.browser_navigate(sandbox.id, "https://httpbin.org/html")
            print(f"   Current URL: {nav.url}")

            # Get clickable elements
            print("\n10. Getting clickable elements...")
            elements = client.web.browser_get_clickable_elements(sandbox.id)
            print(f"   Found {len(elements.elements)} clickable elements")
            if len(elements.elements) > 0:
                print(f"   First element: {elements.elements[0]}")

            # Screenshot
            print("\n11. Taking browser screenshot...")
            screenshot = client.web.browser_screenshot(sandbox.id, name="httpbin_example")
            print(f"   Screenshot saved: {screenshot.path}")

            # JavaScript evaluation
            print("\n12. Evaluating JavaScript...")
            result = client.web.browser_evaluate(
                sandbox.id, script="() => ({title: document.title, url: window.location.href})"
            )
            print(f"   Result: {result.result}")

            # Tab management
            print("\n13. Managing tabs...")
            client.web.browser_new_tab(sandbox.id, url="https://example.com")
            tabs = client.web.browser_tab_list(sandbox.id)
            print(f"    Open tabs: {len(tabs.tabs)}")
            for i, tab in enumerate(tabs.tabs):
                print(f"    - Tab {i}: {tab.title} ({tab.url})")

        else:
            print("   Browser automation not supported in this sandbox")
            print("   Use 'dsb/sandbox' image instead of 'dsb/sandbox-slim'")

        # =====================================================================
        # Health Check
        # =====================================================================
        print("\n" + "=" * 60)
        print("HEALTH CHECK")
        print("=" * 60)

        print("\n14. Checking web tools health...")
        health = client.web.health_check(sandbox.id)
        print(f"   CDP URL: {health.cdp_url}")
        print(f"   Browser ready: {health.browser_ready}")
        print(f"   Message: {health.message}")

        # =====================================================================
        # Cleanup
        # =====================================================================
        print("\n" + "=" * 60)
        print("CLEANUP")
        print("=" * 60)

        print("\n15. Deleting sandbox...")
        client.sandbox.delete(sandbox.id)
        print("   Sandbox deleted successfully")

    except Exception as e:
        print(f"Error: {e}")
        raise
    finally:
        client.close()
        print("\nClient closed.")


if __name__ == "__main__":
    main()
