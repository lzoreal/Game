//! Solver implementations for chaser-cf

use super::BrowserManager;
use crate::error::{ChaserError, ChaserResult};
use crate::models::{Cookie, ProxyConfig, WafSession};

use chaser_oxide::auth::Credentials;
use std::collections::HashMap;
use std::time::Duration;

const FAKE_PAGE_HTML: &str = include_str!("../resources/fake_page.html");

/// Get page source from a Cloudflare-protected URL.
pub async fn get_source(
    manager: &BrowserManager,
    url: &str,
    proxy: Option<ProxyConfig>,
) -> ChaserResult<String> {
    let _permit = manager.acquire_permit().await?;
    let ctx_id = manager.create_context(proxy.as_ref()).await?;
    let (page, chaser) = manager.new_page(ctx_id, "about:blank").await?;

    setup_proxy_auth(&page, proxy.as_ref()).await?;

    chaser
        .goto(url)
        .await
        .map_err(|e| ChaserError::NavigationFailed(e.to_string()))?;

    wait_for_clearance(&page, &chaser, 30).await;

    page.content()
        .await
        .map_err(|e| ChaserError::Internal(e.to_string()))
}

/// Navigate to a Cloudflare-protected URL, solve any challenge, and return the
/// rendered body text. Replaying clearance cookies through a separate HTTP
/// client often fails fingerprint checks, so callers that need the payload
/// should read it from the browser itself.
pub async fn get_body_text(
    manager: &BrowserManager,
    url: &str,
    proxy: Option<ProxyConfig>,
) -> ChaserResult<String> {
    const BODY_CHUNK: u64 = 512 * 1024;
    let debug = std::env::var_os("CHASER_DEBUG").is_some();
    let started = std::time::Instant::now();

    let _permit = manager.acquire_permit().await?;
    let ctx_id = manager.create_context(proxy.as_ref()).await?;
    let (page, chaser) = manager.new_page(ctx_id, "about:blank").await?;

    setup_proxy_auth(&page, proxy.as_ref()).await?;

    chaser
        .goto(url)
        .await
        .map_err(|e| ChaserError::NavigationFailed(e.to_string()))?;
    if debug {
        eprintln!("[chaser] navigated in {:?}", started.elapsed());
    }

    let cleared = wait_for_clearance(&page, &chaser, 90).await;
    if debug {
        eprintln!(
            "[chaser] wait done in {:?} clearance={cleared}",
            started.elapsed()
        );
    }

    let outcome = async {
        if !cleared {
            return Err(ChaserError::CaptchaFailed(
                "timed out waiting for cf_clearance".into(),
            ));
        }

        // Fetch inside the page (browser TLS/cookies/fingerprint). Kick it off
        // asynchronously and poll a small state object so an 18 MB download
        // cannot exceed a single CDP command's timeout.
        chaser
            .evaluate(
                r#"(function() {
                    window.__chaser_state = { done: false, error: null, length: 0 };
                    const load = async (attempt) => {
                        try {
                            const response = await fetch(location.href, {
                                cache: 'default',
                                credentials: 'include'
                            });
                            if (!response.ok) throw new Error('HTTP ' + response.status);
                            window.__chaser_body = await response.text();
                            window.__chaser_state = {
                                done: true,
                                error: null,
                                length: window.__chaser_body.length
                            };
                        } catch (error) {
                            if (attempt < 3) {
                                setTimeout(() => load(attempt + 1), 800);
                            } else {
                                window.__chaser_state = {
                                    done: true,
                                    error: String(error),
                                    length: 0
                                };
                            }
                        }
                    };
                    load(0);
                    return true;
                })()"#,
            )
            .await
            .map_err(|e| ChaserError::Internal(e.to_string()))?;

        let deadline = std::time::Instant::now() + Duration::from_secs(150);
        let length = loop {
            let state = chaser
                .evaluate("window.__chaser_state || null")
                .await
                .map_err(|e| ChaserError::Internal(e.to_string()))?;
            if let Some(state) = state {
                if state
                    .get("done")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false)
                {
                    if let Some(error) = state
                        .get("error")
                        .and_then(|value| value.as_str())
                        .filter(|error| !error.is_empty())
                    {
                        return Err(ChaserError::Internal(error.to_string()));
                    }
                    break state
                        .get("length")
                        .and_then(|value| value.as_u64())
                        .filter(|length| *length > 0)
                        .ok_or_else(|| ChaserError::Internal("浏览器返回了空内容".into()))?;
                }
            }
            if std::time::Instant::now() > deadline {
                return Err(ChaserError::Internal("下载源正文超时".into()));
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
        };
        if debug {
            eprintln!(
                "[chaser] body length {length} at {:?}",
                started.elapsed()
            );
        }

        let mut body = String::with_capacity(length as usize);
        let mut offset = 0u64;
        while offset < length {
            let script = format!(
                "(function() {{
                    const s = window.__chaser_body;
                    let end = Math.min({offset} + {BODY_CHUNK}, s.length);
                    if (end < s.length && end > {offset}
                        && (s.charCodeAt(end - 1) & 0xFC00) === 0xD800) {{
                        end -= 1;
                    }}
                    return [s.slice({offset}, end), end];
                }})()"
            );
            let chunk = chaser
                .evaluate(&script)
                .await
                .map_err(|e| ChaserError::Internal(e.to_string()))?;
            let items = chunk
                .as_ref()
                .and_then(|value| value.as_array())
                .ok_or_else(|| ChaserError::Internal("读取正文分片失败".into()))?;
            let part = items.first().and_then(|value| value.as_str()).unwrap_or("");
            let end = items.get(1).and_then(|value| value.as_u64()).unwrap_or(0);
            if part.is_empty() || end <= offset {
                return Err(ChaserError::Internal("读取正文分片失败".into()));
            }
            body.push_str(part);
            offset = end;
        }

        let _ = chaser
            .evaluate("delete window.__chaser_body; delete window.__chaser_state;")
            .await;
        if debug {
            eprintln!(
                "[chaser] body fetched {} bytes in {:?}",
                body.len(),
                started.elapsed()
            );
        }
        Ok(body)
    }
    .await;

    let _ = page.close().await;
    outcome
}

/// Navigate to a Cloudflare-protected URL with a stealth browser, solve any interactive
/// challenge (including Turnstile managed challenges via CDP shadow-root click), and
/// return the resulting cookies + User-Agent for use in subsequent HTTP requests.
pub async fn solve_waf_session(
    manager: &BrowserManager,
    url: &str,
    proxy: Option<ProxyConfig>,
) -> ChaserResult<WafSession> {
    let _permit = manager.acquire_permit().await?;
    let ctx_id = manager.create_context(proxy.as_ref()).await?;
    let (page, chaser) = manager.new_page(ctx_id, "about:blank").await?;

    setup_proxy_auth(&page, proxy.as_ref()).await?;

    chaser
        .goto(url)
        .await
        .map_err(|e| ChaserError::NavigationFailed(e.to_string()))?;

    wait_for_clearance(&page, &chaser, 90).await;

    let raw_cookies = page
        .get_cookies()
        .await
        .map_err(|e| ChaserError::CookieExtractionFailed(e.to_string()))?;

    let cookies: Vec<Cookie> = raw_cookies
        .into_iter()
        .map(|c| Cookie {
            name: c.name,
            value: c.value,
            domain: Some(c.domain),
            path: Some(c.path),
            expires: Some(c.expires),
            http_only: Some(c.http_only),
            secure: Some(c.secure),
            same_site: c.same_site.map(|s| format!("{s:?}")),
        })
        .collect();

    let user_agent = chaser
        .evaluate("navigator.userAgent")
        .await
        .ok()
        .and_then(|v| v?.as_str().map(str::to_owned))
        .unwrap_or_default();

    let mut headers = HashMap::new();
    headers.insert("user-agent".to_string(), user_agent);

    Ok(WafSession::new(cookies, headers))
}

/// Solve Turnstile with full page load.
pub async fn solve_turnstile_max(
    manager: &BrowserManager,
    url: &str,
    proxy: Option<ProxyConfig>,
) -> ChaserResult<String> {
    let _permit = manager.acquire_permit().await?;
    let ctx_id = manager.create_context(proxy.as_ref()).await?;
    let (page, chaser) = manager.new_page(ctx_id, "about:blank").await?;

    setup_proxy_auth(&page, proxy.as_ref()).await?;

    page.evaluate_on_new_document(TURNSTILE_EXTRACTOR_SCRIPT)
        .await
        .map_err(|e| ChaserError::Internal(e.to_string()))?;

    chaser
        .goto(url)
        .await
        .map_err(|e| ChaserError::NavigationFailed(e.to_string()))?;

    wait_for_turnstile_token(&page, 60).await
}

/// Solve Turnstile with minimal resource usage (request interception mode).
pub async fn solve_turnstile_min(
    manager: &BrowserManager,
    url: &str,
    site_key: &str,
    proxy: Option<ProxyConfig>,
) -> ChaserResult<String> {
    use chaser_oxide::cdp::browser_protocol::fetch::EventRequestPaused;
    use chaser_oxide::cdp::browser_protocol::network::ResourceType;
    use futures::StreamExt;

    let _permit = manager.acquire_permit().await?;
    let ctx_id = manager.create_context(proxy.as_ref()).await?;
    let (page, chaser) = manager.new_page(ctx_id, "about:blank").await?;

    setup_proxy_auth(&page, proxy.as_ref()).await?;

    let fake_html = FAKE_PAGE_HTML.replace("<site-key>", site_key);

    chaser
        .enable_request_interception("*", Some(ResourceType::Document))
        .await
        .map_err(|e| ChaserError::Internal(format!("enable interception: {e}")))?;

    let mut request_events = page
        .event_listener::<EventRequestPaused>()
        .await
        .map_err(|e| ChaserError::Internal(format!("request listener: {e}")))?;

    let url_str = url.to_string();
    let fake_html_clone = fake_html.clone();
    let chaser_clone = chaser.clone();

    let intercept_handle = tokio::spawn(async move {
        while let Some(event) = request_events.next().await {
            let req_url = &event.request.url;
            let is_target = req_url == &url_str
                || req_url == &format!("{}/", url_str)
                || req_url.starts_with(&url_str);

            if is_target && event.resource_type == ResourceType::Document {
                let _ = chaser_clone
                    .fulfill_request_html(event.request_id.clone(), &fake_html_clone, 200)
                    .await;
            } else {
                let _ = chaser_clone
                    .continue_request(event.request_id.clone())
                    .await;
            }
        }
    });

    chaser
        .goto(url)
        .await
        .map_err(|e| ChaserError::NavigationFailed(e.to_string()))?;

    let token = wait_for_turnstile_token(&page, 60).await?;

    intercept_handle.abort();
    let _ = chaser.disable_request_interception().await;

    Ok(token)
}

// ─── helpers ────────────────────────────────────────────────────────────────

async fn setup_proxy_auth(
    page: &chaser_oxide::Page,
    proxy: Option<&ProxyConfig>,
) -> ChaserResult<()> {
    if let Some(p) = proxy {
        if let (Some(username), Some(password)) = (&p.username, &p.password) {
            page.authenticate(Credentials {
                username: username.clone(),
                password: password.clone(),
            })
            .await
            .map_err(|e| ChaserError::Internal(format!("proxy auth: {e}")))?;
        }
    }
    Ok(())
}

/// Poll until `cf_clearance` appears (meaning the challenge was solved) or the
/// timeout expires.
///
/// For the first `PASSIVE_WAIT_MS` milliseconds we do nothing — the CF managed
/// challenge JS runs its invisible PoW/fingerprint in this window. Polling the
/// DOM while it runs causes timing anomalies that raise the bot score. After
/// the passive window we check whether a Turnstile widget has appeared and, if
/// so, click it using the shadow-root CDP traversal.
async fn wait_for_clearance(
    page: &chaser_oxide::Page,
    chaser: &chaser_oxide::ChaserPage,
    timeout_seconds: u64,
) -> bool {
    const PASSIVE_WAIT_MS: u64 = 6_000;
    const CLICK_INTERVAL_MS: u64 = 1_200;
    let debug = std::env::var_os("CHASER_DEBUG").is_some();

    let started = std::time::Instant::now();
    let timeout = Duration::from_secs(timeout_seconds);
    let mut last_click = started - Duration::from_secs(30);

    loop {
        if has_clearance_cookie(page).await {
            if debug {
                eprintln!("[chaser] clearance seen at {:?}", started.elapsed());
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
            return true;
        }

        if started.elapsed() >= timeout {
            if debug {
                eprintln!("[chaser] clearance timeout at {:?}", started.elapsed());
            }
            return false;
        }

        // Only start DOM inspection / clicking after the passive window.
        if started.elapsed().as_millis() as u64 >= PASSIVE_WAIT_MS
            && last_click.elapsed().as_millis() as u64 >= CLICK_INTERVAL_MS
        {
            try_click_challenge(chaser).await;
            last_click = std::time::Instant::now();
        }

        tokio::time::sleep(Duration::from_millis(400)).await;
    }
}

/// Return true if the browser has a `cf_clearance` cookie for any domain.
async fn has_clearance_cookie(page: &chaser_oxide::Page) -> bool {
    page.get_cookies()
        .await
        .map(|cookies| cookies.iter().any(|c| c.name == "cf_clearance"))
        .unwrap_or(false)
}

/// Click the Turnstile challenge element by traversing its closed shadow root via CDP.
///
/// Cloudflare's Turnstile widget lives inside a CLOSED shadow root. JS's
/// `element.shadowRoot` returns null for these, but CDP's `DOM.getDocument` with
/// `pierce: true` exposes them as `node.shadow_roots` — identical to what the Python
/// CF-Clearance-Scraper does with `parent.shadow_roots[0]`.
async fn turnstile_iframe_point(chaser: &chaser_oxide::ChaserPage) -> Option<(f64, f64)> {
    let value = chaser
        .evaluate(
            r#"(function() {
                const frames = Array.from(document.querySelectorAll('iframe'));
                for (const frame of frames) {
                    const src = frame.src || '';
                    if (!src.includes('challenges.cloudflare.com')) continue;
                    const rect = frame.getBoundingClientRect();
                    if (rect.width < 100 || rect.height < 30) continue;
                    return [rect.left + 30, rect.top + rect.height / 2];
                }
                return null;
            })()"#,
        )
        .await
        .ok()
        .flatten()?;
    let items = value.as_array()?;
    let x = items.first()?.as_f64()?;
    let y = items.get(1)?.as_f64()?;
    if x.is_finite() && y.is_finite() {
        Some((x, y))
    } else {
        None
    }
}

async fn try_click_challenge(chaser: &chaser_oxide::ChaserPage) {
    use chaser_oxide::cdp::browser_protocol::dom::{GetBoxModelParams, GetDocumentParams};

    let page = chaser.raw_page();

    // Current Cloudflare Turnstile renders in a cross-origin iframe, which the
    // closed-shadow-root traversal below cannot see. Click the checkbox area of
    // the widget iframe directly.
    if let Some((x, y)) = turnstile_iframe_point(chaser).await {
        if std::env::var_os("CHASER_DEBUG").is_some() {
            eprintln!("[chaser] clicking turnstile iframe at ({x:.1}, {y:.1})");
        }
        let point = chaser_oxide::layout::Point::new(x, y);
        let _ = page.move_mouse(point).await;
        tokio::time::sleep(Duration::from_millis(120)).await;
        let _ = page.click(point).await;
        return;
    }

    let doc = match page
        .execute(GetDocumentParams {
            depth: Some(-1),
            pierce: Some(true),
        })
        .await
    {
        Ok(r) => r,
        Err(_) => return,
    };

    let Some(target_id) = find_shadow_challenge_node(&doc.result.root) else {
        if let Some((x, y)) = challenge_iframe_point(&page, &doc.result.root).await {
            if std::env::var_os("CHASER_DEBUG").is_some() {
                eprintln!("[chaser] clicking shadow iframe at ({x:.1}, {y:.1})");
            }
            let point = chaser_oxide::layout::Point::new(x, y);
            let _ = page.move_mouse(point).await;
            tokio::time::sleep(Duration::from_millis(120)).await;
            let _ = page.click(point).await;
        }
        return;
    };

    let box_model = match page
        .execute(GetBoxModelParams {
            node_id: Some(target_id),
            backend_node_id: None,
            object_id: None,
        })
        .await
    {
        Ok(r) => r,
        Err(_) => return,
    };

    let content = box_model.result.model.content.inner();
    if content.len() < 8 {
        return;
    }

    let cx = (content[0] + content[2]) / 2.0;
    let cy = (content[1] + content[5]) / 2.0;

    // Compute all random values in a synchronous block so ThreadRng is dropped
    // before any await point — ThreadRng is !Send and would poison the future.
    let (tx, ty, curve_points, post_pause_ms) = {
        use rand::Rng as _;
        let mut rng = rand::rng();

        let tx = cx + rng.random_range(-5.0..=5.0_f64);
        let ty = cy + rng.random_range(-4.0..=4.0_f64);

        // Ghost-cursor style: cubic Bezier from a random off-screen origin.
        // P0 = start (random position away from target), P3 = target.
        // P1, P2 are random control points that produce a natural arc.
        let p0x = tx + rng.random_range(-200.0..=-60.0_f64);
        let p0y = ty + rng.random_range(-120.0..=120.0_f64);
        let p1x =
            p0x + (tx - p0x) * rng.random_range(0.2..0.5_f64) + rng.random_range(-30.0..30.0);
        let p1y =
            p0y + (ty - p0y) * rng.random_range(0.1..0.4_f64) + rng.random_range(-40.0..40.0);
        let p2x =
            p0x + (tx - p0x) * rng.random_range(0.5..0.8_f64) + rng.random_range(-20.0..20.0);
        let p2y =
            p0y + (ty - p0y) * rng.random_range(0.5..0.9_f64) + rng.random_range(-20.0..20.0);

        let steps: u8 = rng.random_range(12..22);
        let mut points: Vec<(f64, f64, u64)> = Vec::with_capacity(steps as usize);
        for i in 1..=steps {
            let t = i as f64 / steps as f64;
            let u = 1.0 - t;
            // Cubic Bezier: B(t) = u³P0 + 3u²tP1 + 3ut²P2 + t³P3
            let bx =
                u * u * u * p0x + 3.0 * u * u * t * p1x + 3.0 * u * t * t * p2x + t * t * t * tx;
            let by =
                u * u * u * p0y + 3.0 * u * u * t * p1y + 3.0 * u * t * t * p2y + t * t * t * ty;
            // Slow down near the target (ease-in-out feel).
            let speed = (4.0 * t * (1.0 - t)).max(0.1);
            let step_ms = (rng.random_range(8.0..22.0_f64) / speed) as u64;
            points.push((bx, by, step_ms.min(80)));
        }

        let post_pause_ms = rng.random_range(40..120_u64);
        (tx, ty, points, post_pause_ms)
        // rng dropped here — no !Send value crosses any await below
    };

    for (bx, by, step_ms) in curve_points {
        let _ = page
            .move_mouse(chaser_oxide::layout::Point::new(bx, by))
            .await;
        tokio::time::sleep(Duration::from_millis(step_ms)).await;
    }

    tokio::time::sleep(Duration::from_millis(post_pause_ms)).await;
    let _ = page.click(chaser_oxide::layout::Point::new(tx, ty)).await;
}

async fn challenge_iframe_point(
    page: &chaser_oxide::Page,
    root: &chaser_oxide::cdp::browser_protocol::dom::Node,
) -> Option<(f64, f64)> {
    use chaser_oxide::cdp::browser_protocol::dom::{GetBoxModelParams, NodeId};

    let node_id: NodeId = find_challenge_iframe(root)?;
    let box_model = page
        .execute(GetBoxModelParams {
            node_id: Some(node_id),
            backend_node_id: None,
            object_id: None,
        })
        .await
        .ok()?;
    let content = box_model.result.model.content.inner();
    if content.len() < 8 {
        return None;
    }
    let width = (content[2] - content[0]).abs();
    let height = (content[5] - content[1]).abs();
    if width < 100.0 || height < 30.0 {
        return None;
    }
    Some((content[0] + 30.0, content[1] + height / 2.0))
}

fn find_challenge_iframe(
    node: &chaser_oxide::cdp::browser_protocol::dom::Node,
) -> Option<chaser_oxide::cdp::browser_protocol::dom::NodeId> {
    if node.node_name.eq_ignore_ascii_case("iframe") {
        let attributes = node.attributes.as_deref().unwrap_or(&[]);
        let challenge_src = attributes.chunks(2).any(|pair| {
            pair.len() == 2
                && pair[0].eq_ignore_ascii_case("src")
                && pair[1].contains("challenges.cloudflare.com")
        });
        if challenge_src {
            return Some(node.node_id.clone());
        }
    }
    for child in node.children.as_deref().unwrap_or(&[]) {
        if let Some(id) = find_challenge_iframe(child) {
            return Some(id);
        }
    }
    for shadow_root in node.shadow_roots.as_deref().unwrap_or(&[]) {
        for child in shadow_root.children.as_deref().unwrap_or(&[]) {
            if let Some(id) = find_challenge_iframe(child) {
                return Some(id);
            }
        }
    }
    None
}

/// Walk the CDP DOM tree and return the NodeId to click for the Turnstile challenge.
///
/// Strategy (per Cloudflare's actual DOM layout):
///   1. Find the node whose DIRECT children include `<input name="cf-turnstile-response">`.
///   2. That node is the shadow host — check its `shadow_roots[0]` (CLOSED shadow root,
///      invisible to JS but exposed by CDP's `pierce: true`).
///   3. Return the first child of the shadow root that is NOT `display:none` — that is
///      the visible challenge element whose centre we click.
fn find_shadow_challenge_node(
    node: &chaser_oxide::cdp::browser_protocol::dom::Node,
) -> Option<chaser_oxide::cdp::browser_protocol::dom::NodeId> {
    let children = node.children.as_deref().unwrap_or(&[]);

    // Is this node the shadow host? (has a cf-turnstile-response input as a direct child)
    let is_shadow_host = children.iter().any(|child| {
        child
            .attributes
            .as_deref()
            .unwrap_or(&[])
            .chunks(2)
            .any(|p| p.len() == 2 && p[0] == "name" && p[1] == "cf-turnstile-response")
    });

    if is_shadow_host {
        if let Some(sr) = node.shadow_roots.as_ref().and_then(|srs| srs.first()) {
            for sr_child in sr.children.as_deref().unwrap_or(&[]) {
                let hidden = sr_child
                    .attributes
                    .as_deref()
                    .unwrap_or(&[])
                    .chunks(2)
                    .any(|p| p.len() == 2 && p[0] == "style" && p[1].contains("display: none"));
                if !hidden {
                    return Some(sr_child.node_id);
                }
            }
        }
    }

    // Recurse into children and shadow roots
    for child in children {
        if let Some(id) = find_shadow_challenge_node(child) {
            return Some(id);
        }
    }
    for sr in node.shadow_roots.as_deref().unwrap_or(&[]) {
        for sr_child in sr.children.as_deref().unwrap_or(&[]) {
            if let Some(id) = find_shadow_challenge_node(sr_child) {
                return Some(id);
            }
        }
    }
    None
}

const TURNSTILE_EXTRACTOR_SCRIPT: &str = r#"
    (function() {
        let token = null;
        async function waitForToken() {
            while (!token) {
                try { token = window.turnstile.getResponse(); } catch(e) {}
                await new Promise(r => setTimeout(r, 500));
            }
            var c = document.createElement("input");
            c.type = "hidden"; c.name = "cf-response"; c.value = token;
            document.body.appendChild(c);
        }
        waitForToken();
    })();
"#;

async fn wait_for_turnstile_token(
    page: &chaser_oxide::Page,
    timeout_seconds: u64,
) -> ChaserResult<String> {
    let start = std::time::Instant::now();
    let timeout = Duration::from_secs(timeout_seconds);
    let chaser = chaser_oxide::ChaserPage::new(page.clone());

    loop {
        if start.elapsed() > timeout {
            return Err(ChaserError::CaptchaFailed(
                "timeout waiting for token".into(),
            ));
        }

        let result = chaser
            .evaluate(
                r#"(function() {
                    if (window.turnstile && typeof window.turnstile.getResponse === 'function') {
                        var t = window.turnstile.getResponse();
                        if (t) return t;
                    }
                    var el = document.querySelector('[name="cf-response"]');
                    return el ? el.value : null;
                })()"#,
            )
            .await;

        if let Ok(Some(v)) = result {
            if let Some(t) = v.as_str() {
                if t.len() > 10 {
                    return Ok(t.to_string());
                }
            }
        }

        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}
