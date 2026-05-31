#!/usr/bin/env /opt/homebrew/bin/python3.10
"""Comprehensive MCP server tool tests."""
import asyncio, json, sys, time
from mcp import ClientSession
from mcp.client.streamable_http import streamablehttp_client

DSB_SERVER = "http://localhost:8081"
MCP_SERVER = "http://localhost:3001"
SESSION_ID = f"test-session-{int(time.time())}"

# ── helpers ─────────────────────────────────────────

async def mcp_call(service_path, tool_name, arguments):
    """Call an MCP tool on the given service endpoint."""
    url = f"{MCP_SERVER}{service_path}"
    async with streamablehttp_client(url) as (read, write, _):
        async with ClientSession(read, write) as session:
            await session.initialize()
            result = await session.call_tool(tool_name, arguments)
            return result

# ── Test 1: Create sandbox ─────────────────────────

async def test_create_sandbox():
    print("=" * 60)
    print("TEST 1: Create sandbox via MCP")
    print("=" * 60)
    result = await mcp_call("/mcp/dsb/sandbox", "create_sandbox", {
        "session_id": SESSION_ID,
        "image": "ghcr.io/dsb/sandbox:k8s-v0.2.1",
        "timeout_minutes": 30,
    })
    print("Create sandbox result:")
    for c in result.content:
        print(f"  {c.text[:500] if hasattr(c, 'text') else c}")
    return result

# ── Test 2: Web Fetch - Douban Top 250 ─────────────

async def test_web_fetch_douban():
    print("\n" + "=" * 60)
    print("TEST 2: Web Fetch - Douban Top 250")
    print("=" * 60)
    result = await mcp_call("/mcp/dsb/web", "web_fetch", {
        "url": "https://movie.douban.com/top250",
        "session_id": SESSION_ID,
        "format": "markdown",
        "max_length": 50000,
    })
    text = None
    for c in result.content:
        if hasattr(c, 'text'):
            text = c.text
            print(f"  Response length: {len(text)} chars")
            print(f"  First 500 chars:\n{text[:500]}")
    return text

# ── Test 3: Web Search ─────────────────────────────

async def test_web_search():
    print("\n" + "=" * 60)
    print("TEST 3: Web Search - Real questions")
    print("=" * 60)
    queries = [
        "Rust programming language latest version 2026",
        "Kubernetes best practices 2026",
        "MCP protocol model context protocol specification",
    ]
    results = []
    for query in queries:
        print(f"\n  Searching: '{query}'")
        result = await mcp_call("/mcp/dsb/web", "web_search", {
            "query": query,
            "result_num": 5,
            "timeout": 30,
        })
        for c in result.content:
            if hasattr(c, 'text'):
                text = c.text
                print(f"  Results ({len(text)} chars): {text[:300]}...")
                results.append(text)
    return results

# ── Test 4: Browser tools - Wikipedia pages ────────

async def test_browser_wikipedia():
    print("\n" + "=" * 60)
    print("TEST 4: Browser tools - Wikipedia pages")
    print("=" * 60)

    pages = [
        "https://en.wikipedia.org/wiki/Rust_(programming_language)",
        "https://en.wikipedia.org/wiki/Kubernetes",
        "https://en.wikipedia.org/wiki/World_Wide_Web",
    ]

    # First navigate
    for i, url in enumerate(pages):
        prefix = f"  [{i+1}/{len(pages)}]"
        print(f"{prefix} Navigating to: {url.split('/')[-1][:50]}...")
        result = await mcp_call("/mcp/dsb/browser", "browser_navigate", {
            "session_id": SESSION_ID,
            "url": url,
        })
        for c in result.content:
            if hasattr(c, 'text'):
                print(f"      Result: {c.text[:200]}")

        # Get clickable elements
        result = await mcp_call("/mcp/dsb/browser", "browser_get_clickable_elements", {
            "session_id": SESSION_ID,
        })
        element_count = 0
        for c in result.content:
            if hasattr(c, 'text'):
                try:
                    data = json.loads(c.text)
                    element_count = len(data) if isinstance(data, list) else 0
                except:
                    element_count = len(c.text)
        print(f"      Found ~{element_count} interactive elements")

        # Take screenshot
        result = await mcp_call("/mcp/dsb/browser", "browser_screenshot", {
            "session_id": SESSION_ID,
            "full_page": False,
        })
        for c in result.content:
            if hasattr(c, 'text'):
                print(f"      Screenshot: {c.text[:200]}")

    return True

# ── Test 5: Concurrent browser requests ────────────

async def test_concurrent_browser():
    print("\n" + "=" * 60)
    print("TEST 5: Concurrent browser requests")
    print("=" * 60)

    async def fetch_page(url, idx):
        result = await mcp_call("/mcp/dsb/browser", "browser_navigate", {
            "session_id": SESSION_ID,
            "url": url,
        })
        # After navigate, evaluate to get title
        result = await mcp_call("/mcp/dsb/browser", "browser_evaluate", {
            "session_id": SESSION_ID,
            "script": "document.title",
        })
        for c in result.content:
            if hasattr(c, 'text'):
                print(f"  [{idx}] Title: {c.text[:150]}")
                return c.text

    urls = [
        "https://en.wikipedia.org/wiki/Artificial_intelligence",
        "https://en.wikipedia.org/wiki/Quantum_computing",
    ]

    tasks = [fetch_page(url, i+1) for i, url in enumerate(urls)]
    results = await asyncio.gather(*tasks, return_exceptions=True)
    for i, r in enumerate(results):
        if isinstance(r, Exception):
            print(f"  [{i+1}] ERROR: {r}")
    return results

# ── Test 6: Generate HTML from Douban results ──────

async def test_generate_douban_html(douban_text):
    print("\n" + "=" * 60)
    print("TEST 6: Generate HTML from Douban results")
    print("=" * 60)

    if not douban_text or len(douban_text) < 100:
        print("  No douban data available - using web_search fallback")
        result = await mcp_call("/mcp/dsb/web", "web_search", {
            "query": "douban top 250 movies list 2025 2026",
            "result_num": 10,
            "timeout": 30,
        })
        for c in result.content:
            if hasattr(c, 'text'):
                douban_text = c.text

    html_content = f"""<!DOCTYPE html>
<html lang="zh-CN">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>豆瓣 Top 250 电影 - DSB MCP Server Test</title>
    <style>
        * {{ margin: 0; padding: 0; box-sizing: border-box; }}
        body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
               background: linear-gradient(135deg, #667eea 0%, #764ba2 100%); min-height: 100vh; padding: 2rem; }}
        .container {{ max-width: 1200px; margin: 0 auto; }}
        h1 {{ color: white; text-align: center; font-size: 2.5rem; margin-bottom: 0.5rem; text-shadow: 2px 2px 4px rgba(0,0,0,0.3); }}
        .subtitle {{ color: rgba(255,255,255,0.8); text-align: center; margin-bottom: 2rem; }}
        .card {{ background: white; border-radius: 12px; padding: 2rem; box-shadow: 0 10px 40px rgba(0,0,0,0.2); margin-bottom: 1.5rem; }}
        .card h2 {{ color: #667eea; margin-bottom: 1rem; }}
        .raw-data {{ background: #f5f5f5; border-radius: 8px; padding: 1.5rem; font-family: 'SF Mono', monospace;
                     font-size: 0.85rem; white-space: pre-wrap; max-height: 600px; overflow-y: auto; line-height: 1.6; }}
        .footer {{ text-align: center; color: rgba(255,255,255,0.6); margin-top: 2rem; font-size: 0.9rem; }}
        .badge {{ display: inline-block; background: #667eea; color: white; padding: 0.25rem 0.75rem;
                 border-radius: 20px; font-size: 0.8rem; margin-right: 0.5rem; }}
    </style>
</head>
<body>
    <div class="container">
        <h1>豆瓣电影 Top 250</h1>
        <p class="subtitle">
            <span class="badge">DSB MCP Server</span>
            <span class="badge">Web Fetch Tool</span>
            <span class="badge">Kubernetes Backend</span>
        </p>
        <div class="card">
            <h2>抓取结果</h2>
            <p style="color:#666;margin-bottom:1rem;">
                以下数据通过 DSB MCP Server 的 <code>web_fetch</code> 工具从 movie.douban.com 实时获取。
                沙箱运行在 Kubernetes 集群上 (YOUR_CLUSTER_NAME)，使用 ghcr.io/dsb/sandbox:k8s-v0.2.1 镜像。
            </p>
            <div class="raw-data">{douban_text}</div>
        </div>
        <div class="card">
            <h2>技术说明</h2>
            <ul style="line-height:2; color:#444;">
                <li>后端: Kubernetes (EKS YOUR_CLUSTER_NAME)</li>
                <li>沙箱镜像: ghcr.io/dsb/sandbox:k8s-v0.2.1</li>
                <li>工具: MCP web_fetch (浏览器渲染 + HTML 转 Markdown)</li>
                <li>认证: Session Cookie (dsb_session) + API Key</li>
                <li>代理: YOUR_PROXY_HOST:3128 (egress)</li>
                <li>生成时间: {time.strftime('%Y-%m-%d %H:%M:%S UTC')}</li>
            </ul>
        </div>
        <p class="footer">Generated by DSB MCP Server Verification Test • Tom Xie</p>
    </div>
</body>
</html>"""

    # Save to file
    output_path = "/Users/tom/src/dsb/douban_top250.html"
    with open(output_path, 'w', encoding='utf-8') as f:
        f.write(html_content)
    print(f"  HTML saved to: {output_path}")
    print(f"  File size: {len(html_content)} bytes")
    return output_path

# ── Main ───────────────────────────────────────────

async def main():
    print("DSB MCP Server Tool Verification")
    print(f"Session ID: {SESSION_ID}")
    print(f"MCP Server: {MCP_SERVER}")
    print(f"DSB Server: {DSB_SERVER}")
    print()

    try:
        # Test 1: Create sandbox
        await test_create_sandbox()

        # Test 2: Fetch Douban Top 250
        douban_text = await test_web_fetch_douban()

        # Test 3: Web search
        await test_web_search()

        # Test 4: Browser with Wikipedia
        await test_browser_wikipedia()

        # Test 5: Concurrent browser
        await test_concurrent_browser()

        # Test 6: Generate HTML
        html_path = await test_generate_douban_html(douban_text)

        print("\n" + "=" * 60)
        print("ALL TESTS COMPLETE")
        print(f"HTML output: {html_path}")
        print("=" * 60)

    except Exception as e:
        print(f"\nERROR: {e}")
        import traceback
        traceback.print_exc()
        sys.exit(1)

if __name__ == "__main__":
    asyncio.run(main())
