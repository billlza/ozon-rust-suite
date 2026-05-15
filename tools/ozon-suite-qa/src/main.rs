use std::{
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow};
use chrono::Utc;
use clap::{Args, Parser, Subcommand, ValueEnum};
use reqwest::{Client, Method, StatusCode};
use serde::Serialize;
use serde_json::{Value, json};
use tokio::{task::JoinSet, time::sleep};

#[derive(Debug, Parser)]
#[command(author, version, about = "Read-only Ozon Rust Suite QA harness")]
struct Cli {
    #[arg(long, default_value = "http://127.0.0.1:8790")]
    base_url: String,
    #[arg(long)]
    local_token: Option<String>,
    #[arg(long)]
    openclaw_token: Option<String>,
    #[command(subcommand)]
    command: CommandKind,
}

#[derive(Debug, Subcommand)]
enum CommandKind {
    Smoke(SmokeArgs),
    Perf(PerfArgs),
    Stability(StabilityArgs),
    Memory(MemoryArgs),
    All(AllArgs),
}

#[derive(Debug, Args)]
struct SmokeArgs {
    #[arg(long)]
    offer_id: Option<String>,
    #[arg(long)]
    product_id: Option<String>,
    #[arg(long)]
    sku: Option<String>,
    #[arg(long, default_value_t = 3)]
    list_limit: u16,
}

#[derive(Debug, Args, Clone)]
struct PerfArgs {
    #[arg(long, value_enum, default_value_t = Scenario::Health)]
    scenario: Scenario,
    #[arg(long, default_value_t = 100)]
    requests: usize,
    #[arg(long, default_value_t = 8)]
    concurrency: usize,
    #[arg(long, default_value_t = 3)]
    list_limit: u16,
    #[arg(long)]
    offer_id: Option<String>,
    #[arg(long)]
    product_id: Option<String>,
    #[arg(long)]
    sku: Option<String>,
}

#[derive(Debug, Args, Clone)]
struct StabilityArgs {
    #[arg(long, value_enum, default_value_t = Scenario::Health)]
    scenario: Scenario,
    #[arg(long, default_value_t = 60)]
    duration_secs: u64,
    #[arg(long, default_value_t = 500)]
    interval_ms: u64,
    #[arg(long, default_value_t = 3)]
    list_limit: u16,
    #[arg(long)]
    offer_id: Option<String>,
    #[arg(long)]
    product_id: Option<String>,
    #[arg(long)]
    sku: Option<String>,
}

#[derive(Debug, Args, Clone)]
struct MemoryArgs {
    #[arg(long)]
    pid: u32,
    #[arg(long, value_enum, default_value_t = Scenario::Health)]
    scenario: Scenario,
    #[arg(long, default_value_t = 120)]
    duration_secs: u64,
    #[arg(long, default_value_t = 1000)]
    interval_ms: u64,
    #[arg(long, default_value_t = 64.0)]
    growth_limit_mb: f64,
    #[arg(long, default_value_t = 3)]
    list_limit: u16,
    #[arg(long)]
    offer_id: Option<String>,
    #[arg(long)]
    product_id: Option<String>,
    #[arg(long)]
    sku: Option<String>,
}

#[derive(Debug, Args, Clone)]
struct AllArgs {
    #[arg(long)]
    pid: Option<u32>,
    #[arg(long)]
    offer_id: Option<String>,
    #[arg(long)]
    product_id: Option<String>,
    #[arg(long)]
    sku: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
enum Scenario {
    Health,
    Manifest,
    ProductCount,
    ProductList,
    ProductGet,
    PosterHandoff,
}

#[derive(Clone)]
struct Runner {
    base_url: String,
    local_token: String,
    openclaw_token: String,
    client: Client,
}

#[derive(Debug, Serialize)]
struct CommandReport {
    command: &'static str,
    ok: bool,
    started_at: String,
    finished_at: String,
    duration_ms: u128,
    steps: Vec<StepReport>,
    metrics: Option<MetricsReport>,
    memory: Option<MemoryReport>,
    summary: String,
}

#[derive(Debug, Serialize)]
struct StepReport {
    name: String,
    ok: bool,
    status: Option<u16>,
    elapsed_ms: u128,
    detail: String,
}

#[derive(Debug, Serialize)]
struct MetricsReport {
    scenario: Scenario,
    requests: usize,
    concurrency: usize,
    success: usize,
    failed: usize,
    elapsed_ms: u128,
    requests_per_sec: f64,
    min_ms: u128,
    p50_ms: u128,
    p95_ms: u128,
    p99_ms: u128,
    max_ms: u128,
}

#[derive(Debug, Serialize)]
struct MemoryReport {
    pid: u32,
    samples: usize,
    start_rss_mb: f64,
    end_rss_mb: f64,
    peak_rss_mb: f64,
    growth_mb: f64,
    growth_limit_mb: f64,
}

#[derive(Clone, Debug)]
struct Lookup {
    offer_id: Option<String>,
    product_id: Option<String>,
    sku: Option<String>,
}

#[derive(Debug)]
struct ProbeResult {
    status: StatusCode,
    body: Value,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let runner = Runner {
        base_url: cli.base_url.trim_end_matches('/').to_string(),
        local_token: cli
            .local_token
            .or_else(|| std::env::var("OZON_LOCAL_TOKEN").ok())
            .unwrap_or_else(|| "dev-local-token".to_string()),
        openclaw_token: cli
            .openclaw_token
            .or_else(|| std::env::var("OZON_OPENCLAW_TOKEN").ok())
            .unwrap_or_else(|| "dev-openclaw-token".to_string()),
        client: Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .context("failed to build HTTP client")?,
    };

    let report = match cli.command {
        CommandKind::Smoke(args) => smoke(&runner, args).await?,
        CommandKind::Perf(args) => perf(&runner, args).await?,
        CommandKind::Stability(args) => stability(&runner, args).await?,
        CommandKind::Memory(args) => memory(&runner, args).await?,
        CommandKind::All(args) => all(&runner, args).await?,
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    if report.ok {
        Ok(())
    } else {
        Err(anyhow!("{} failed: {}", report.command, report.summary))
    }
}

async fn smoke(runner: &Runner, args: SmokeArgs) -> Result<CommandReport> {
    let started_at = Utc::now().to_rfc3339();
    let started = Instant::now();
    let mut steps = Vec::new();

    let health = run_step("health", || runner.get("/health", Auth::None)).await;
    steps.push(health);
    let manifest = run_step("openclaw.manifest", || {
        runner.get("/openclaw/manifest", Auth::None)
    })
    .await;
    steps.push(manifest);
    let config = run_step("config.status", || {
        runner.get("/config/status", Auth::Local)
    })
    .await;
    steps.push(config);

    let product_count = run_step("ozon.products.count", || {
        runner.post("/tools/ozon.products.count", Auth::Bridge, json!({}))
    })
    .await;
    steps.push(product_count);

    let product_list = runner
        .post(
            "/tools/ozon.products.list",
            Auth::Bridge,
            json!({ "limit": args.list_limit }),
        )
        .await;
    let mut lookup = explicit_lookup(args.offer_id, args.product_id, args.sku);
    match product_list {
        Ok(result) => {
            let sample = result
                .body
                .get("products")
                .and_then(Value::as_array)
                .and_then(|items| items.first())
                .and_then(|item| item.get("offer_id"))
                .and_then(Value::as_str)
                .map(|offer_id| Lookup {
                    offer_id: Some(offer_id.to_string()),
                    product_id: None,
                    sku: None,
                });
            if lookup.is_none() {
                lookup = sample;
            }
            steps.push(StepReport {
                name: "ozon.products.list".to_string(),
                ok: result.status.is_success(),
                status: Some(result.status.as_u16()),
                elapsed_ms: 0,
                detail: summarize_body(&result.body),
            });
        }
        Err(error) => steps.push(StepReport {
            name: "ozon.products.list".to_string(),
            ok: false,
            status: None,
            elapsed_ms: 0,
            detail: error.to_string(),
        }),
    }

    if let Some(lookup) = lookup {
        let product_get = run_step("ozon.products.get", || {
            runner.post(
                "/tools/ozon.products.get",
                Auth::Bridge,
                lookup_json(&lookup),
            )
        })
        .await;
        steps.push(product_get);
        let handoff = run_step("poster.handoff", || {
            runner.post(
                "/poster/handoff",
                Auth::Bridge,
                poster_lookup_json(&lookup, "studio"),
            )
        })
        .await;
        steps.push(handoff);
    } else {
        steps.push(StepReport {
            name: "poster.handoff".to_string(),
            ok: false,
            status: None,
            elapsed_ms: 0,
            detail: "no explicit lookup and product list returned no sample offer_id".to_string(),
        });
    }

    Ok(report(
        "smoke",
        started_at,
        started,
        steps,
        None,
        None,
        "read-only smoke checks completed",
    ))
}

async fn perf(runner: &Runner, args: PerfArgs) -> Result<CommandReport> {
    let started_at = Utc::now().to_rfc3339();
    let started = Instant::now();
    let metrics = run_perf(
        runner,
        args.scenario,
        args.requests,
        args.concurrency.max(1),
        scenario_input(args.list_limit, args.offer_id, args.product_id, args.sku),
    )
    .await?;
    let ok = metrics.failed == 0;
    Ok(CommandReport {
        command: "perf",
        ok,
        started_at,
        finished_at: Utc::now().to_rfc3339(),
        duration_ms: started.elapsed().as_millis(),
        steps: Vec::new(),
        metrics: Some(metrics),
        memory: None,
        summary: if ok {
            "performance run completed without HTTP failures".to_string()
        } else {
            "performance run saw HTTP failures".to_string()
        },
    })
}

async fn stability(runner: &Runner, args: StabilityArgs) -> Result<CommandReport> {
    let started_at = Utc::now().to_rfc3339();
    let started = Instant::now();
    let input = scenario_input(args.list_limit, args.offer_id, args.product_id, args.sku);
    let deadline = Instant::now() + Duration::from_secs(args.duration_secs);
    let mut samples = Vec::new();
    let mut failures = 0usize;
    while Instant::now() < deadline {
        let sample = run_scenario(runner, args.scenario, &input).await;
        if !sample.ok {
            failures += 1;
        }
        samples.push(sample);
        sleep(Duration::from_millis(args.interval_ms)).await;
    }
    let metrics = metrics_from_samples(args.scenario, samples, 1, started.elapsed());
    let ok = failures == 0;
    Ok(CommandReport {
        command: "stability",
        ok,
        started_at,
        finished_at: Utc::now().to_rfc3339(),
        duration_ms: started.elapsed().as_millis(),
        steps: Vec::new(),
        metrics: Some(metrics),
        memory: None,
        summary: if ok {
            "stability run completed without HTTP failures".to_string()
        } else {
            format!("stability run saw {failures} failed cycles")
        },
    })
}

async fn memory(runner: &Runner, args: MemoryArgs) -> Result<CommandReport> {
    let started_at = Utc::now().to_rfc3339();
    let started = Instant::now();
    let input = scenario_input(args.list_limit, args.offer_id, args.product_id, args.sku);
    let deadline = Instant::now() + Duration::from_secs(args.duration_secs);
    let mut rss_samples = Vec::new();
    let mut request_samples = Vec::new();
    while Instant::now() < deadline {
        rss_samples.push(read_rss_kib(args.pid).context("failed to sample target RSS")?);
        let sample = run_scenario(runner, args.scenario, &input).await;
        request_samples.push(sample);
        sleep(Duration::from_millis(args.interval_ms)).await;
    }
    rss_samples.push(read_rss_kib(args.pid).context("failed to sample target RSS")?);
    let memory = memory_report(args.pid, &rss_samples, args.growth_limit_mb);
    let metrics = metrics_from_samples(args.scenario, request_samples, 1, started.elapsed());
    let ok = metrics.failed == 0 && memory.growth_mb <= args.growth_limit_mb;
    Ok(CommandReport {
        command: "memory",
        ok,
        started_at,
        finished_at: Utc::now().to_rfc3339(),
        duration_ms: started.elapsed().as_millis(),
        steps: Vec::new(),
        metrics: Some(metrics),
        memory: Some(memory),
        summary: if ok {
            "memory run stayed within RSS growth limit".to_string()
        } else {
            "memory run failed HTTP checks or exceeded RSS growth limit".to_string()
        },
    })
}

async fn all(runner: &Runner, args: AllArgs) -> Result<CommandReport> {
    let started_at = Utc::now().to_rfc3339();
    let started = Instant::now();
    let mut steps = Vec::new();

    let smoke_report = smoke(
        runner,
        SmokeArgs {
            offer_id: args.offer_id.clone(),
            product_id: args.product_id.clone(),
            sku: args.sku.clone(),
            list_limit: 3,
        },
    )
    .await?;
    steps.extend(smoke_report.steps);

    let perf_report = perf(
        runner,
        PerfArgs {
            scenario: Scenario::Health,
            requests: 30,
            concurrency: 4,
            list_limit: 3,
            offer_id: args.offer_id.clone(),
            product_id: args.product_id.clone(),
            sku: args.sku.clone(),
        },
    )
    .await?;

    let stability_report = stability(
        runner,
        StabilityArgs {
            scenario: Scenario::Health,
            duration_secs: 10,
            interval_ms: 500,
            list_limit: 3,
            offer_id: args.offer_id.clone(),
            product_id: args.product_id.clone(),
            sku: args.sku.clone(),
        },
    )
    .await?;

    let mut memory_report_value = None;
    if let Some(pid) = args.pid {
        let memory_report = memory(
            runner,
            MemoryArgs {
                pid,
                scenario: Scenario::Health,
                duration_secs: 10,
                interval_ms: 500,
                growth_limit_mb: 64.0,
                list_limit: 3,
                offer_id: args.offer_id,
                product_id: args.product_id,
                sku: args.sku,
            },
        )
        .await?;
        memory_report_value = memory_report.memory;
    }

    let ok = steps.iter().all(|step| step.ok)
        && perf_report.ok
        && stability_report.ok
        && memory_report_value
            .as_ref()
            .is_none_or(|memory| memory.growth_mb <= memory.growth_limit_mb);
    Ok(CommandReport {
        command: "all",
        ok,
        started_at,
        finished_at: Utc::now().to_rfc3339(),
        duration_ms: started.elapsed().as_millis(),
        steps,
        metrics: perf_report.metrics,
        memory: memory_report_value,
        summary: if ok {
            "combined smoke, perf, stability, and optional memory checks passed".to_string()
        } else {
            "one or more combined checks failed".to_string()
        },
    })
}

async fn run_perf(
    runner: &Runner,
    scenario: Scenario,
    requests: usize,
    concurrency: usize,
    input: ScenarioInput,
) -> Result<MetricsReport> {
    let started = Instant::now();
    let counter = Arc::new(AtomicUsize::new(0));
    let mut join_set = JoinSet::new();
    for _ in 0..concurrency {
        let runner = runner.clone();
        let counter = Arc::clone(&counter);
        let input = input.clone();
        join_set.spawn(async move {
            let mut samples = Vec::new();
            loop {
                let next = counter.fetch_add(1, Ordering::Relaxed);
                if next >= requests {
                    break;
                }
                samples.push(run_scenario(&runner, scenario, &input).await);
            }
            samples
        });
    }
    let mut samples = Vec::new();
    while let Some(result) = join_set.join_next().await {
        samples.extend(result.context("perf worker failed")?);
    }
    Ok(metrics_from_samples(
        scenario,
        samples,
        concurrency,
        started.elapsed(),
    ))
}

async fn run_step<F, Fut>(name: &str, f: F) -> StepReport
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<ProbeResult>>,
{
    let started = Instant::now();
    match f().await {
        Ok(result) => StepReport {
            name: name.to_string(),
            ok: result.status.is_success(),
            status: Some(result.status.as_u16()),
            elapsed_ms: started.elapsed().as_millis(),
            detail: summarize_body(&result.body),
        },
        Err(error) => StepReport {
            name: name.to_string(),
            ok: false,
            status: None,
            elapsed_ms: started.elapsed().as_millis(),
            detail: error.to_string(),
        },
    }
}

async fn run_scenario(runner: &Runner, scenario: Scenario, input: &ScenarioInput) -> RequestSample {
    let started = Instant::now();
    let response = match scenario {
        Scenario::Health => runner.get("/health", Auth::None).await,
        Scenario::Manifest => runner.get("/openclaw/manifest", Auth::None).await,
        Scenario::ProductCount => {
            runner
                .post("/tools/ozon.products.count", Auth::Bridge, json!({}))
                .await
        }
        Scenario::ProductList => {
            runner
                .post(
                    "/tools/ozon.products.list",
                    Auth::Bridge,
                    json!({ "limit": input.list_limit }),
                )
                .await
        }
        Scenario::ProductGet => {
            if let Some(lookup) = input.lookup.as_ref() {
                runner
                    .post(
                        "/tools/ozon.products.get",
                        Auth::Bridge,
                        lookup_json(lookup),
                    )
                    .await
            } else {
                Err(anyhow!(
                    "product_get scenario requires --offer-id, --product-id, or --sku"
                ))
            }
        }
        Scenario::PosterHandoff => {
            if let Some(lookup) = input.lookup.as_ref() {
                runner
                    .post(
                        "/poster/handoff",
                        Auth::Bridge,
                        poster_lookup_json(lookup, "studio"),
                    )
                    .await
            } else {
                Err(anyhow!(
                    "poster_handoff scenario requires --offer-id, --product-id, or --sku"
                ))
            }
        }
    };
    match response {
        Ok(result) => RequestSample {
            ok: result.status.is_success(),
            elapsed_ms: started.elapsed().as_millis(),
        },
        Err(_) => RequestSample {
            ok: false,
            elapsed_ms: started.elapsed().as_millis(),
        },
    }
}

#[derive(Clone)]
struct ScenarioInput {
    list_limit: u16,
    lookup: Option<Lookup>,
}

#[derive(Debug)]
struct RequestSample {
    ok: bool,
    elapsed_ms: u128,
}

fn scenario_input(
    list_limit: u16,
    offer_id: Option<String>,
    product_id: Option<String>,
    sku: Option<String>,
) -> ScenarioInput {
    ScenarioInput {
        list_limit,
        lookup: explicit_lookup(offer_id, product_id, sku),
    }
}

impl Runner {
    async fn get(&self, path: &str, auth: Auth) -> Result<ProbeResult> {
        self.request(Method::GET, path, auth, None).await
    }

    async fn post(&self, path: &str, auth: Auth, body: Value) -> Result<ProbeResult> {
        self.request(Method::POST, path, auth, Some(body)).await
    }

    async fn request(
        &self,
        method: Method,
        path: &str,
        auth: Auth,
        body: Option<Value>,
    ) -> Result<ProbeResult> {
        let url = format!("{}{}", self.base_url, path);
        let mut request = self.client.request(method, url);
        match auth {
            Auth::None => {}
            Auth::Local => {
                request = request.header("x-local-token", &self.local_token);
            }
            Auth::Bridge => {
                request = request.header("x-openclaw-token", &self.openclaw_token);
            }
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await?;
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        let body = serde_json::from_str(&text).unwrap_or_else(|_| json!({ "raw": text }));
        Ok(ProbeResult { status, body })
    }
}

#[derive(Clone, Copy)]
enum Auth {
    None,
    Local,
    Bridge,
}

fn explicit_lookup(
    offer_id: Option<String>,
    product_id: Option<String>,
    sku: Option<String>,
) -> Option<Lookup> {
    let filled = [offer_id.is_some(), product_id.is_some(), sku.is_some()]
        .into_iter()
        .filter(|value| *value)
        .count();
    if filled == 0 {
        return None;
    }
    Some(Lookup {
        offer_id,
        product_id,
        sku,
    })
}

fn lookup_json(lookup: &Lookup) -> Value {
    let mut value = serde_json::Map::new();
    if let Some(offer_id) = lookup.offer_id.as_deref() {
        value.insert("offer_id".to_string(), json!(offer_id));
    }
    if let Some(product_id) = lookup.product_id.as_deref() {
        value.insert("product_id".to_string(), json!(product_id));
    }
    if let Some(sku) = lookup.sku.as_deref() {
        value.insert("sku".to_string(), json!(sku));
    }
    Value::Object(value)
}

fn poster_lookup_json(lookup: &Lookup, theme: &str) -> Value {
    let mut value = match lookup_json(lookup) {
        Value::Object(value) => value,
        _ => serde_json::Map::new(),
    };
    value.insert("theme".to_string(), json!(theme));
    value.insert("locale".to_string(), json!("zh-CN"));
    Value::Object(value)
}

fn summarize_body(body: &Value) -> String {
    if let Some(error) = body.get("error").and_then(Value::as_str) {
        return error.to_string();
    }
    if let Some(status) = body.get("status").and_then(Value::as_str) {
        return format!("status={status}");
    }
    if let Some(total) = body.get("total").and_then(Value::as_u64) {
        return format!("total={total}");
    }
    if let Some(count) = body.get("count").and_then(Value::as_u64) {
        return format!("count={count}");
    }
    if let Some(product) = body.get("product") {
        let offer_id = product
            .get("offer_id")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let images = product
            .get("images")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        return format!("offer_id={offer_id}; images={images}");
    }
    if let Some(prompt) = body.get("prompt").and_then(Value::as_str) {
        return format!("prompt_chars={}", prompt.chars().count());
    }
    "ok".to_string()
}

fn report(
    command: &'static str,
    started_at: String,
    started: Instant,
    steps: Vec<StepReport>,
    metrics: Option<MetricsReport>,
    memory: Option<MemoryReport>,
    ok_summary: &str,
) -> CommandReport {
    let ok = steps.iter().all(|step| step.ok)
        && metrics.as_ref().is_none_or(|value| value.failed == 0)
        && memory
            .as_ref()
            .is_none_or(|value| value.growth_mb <= value.growth_limit_mb);
    CommandReport {
        command,
        ok,
        started_at,
        finished_at: Utc::now().to_rfc3339(),
        duration_ms: started.elapsed().as_millis(),
        steps,
        metrics,
        memory,
        summary: if ok {
            ok_summary.to_string()
        } else {
            "one or more checks failed".to_string()
        },
    }
}

fn metrics_from_samples(
    scenario: Scenario,
    samples: Vec<RequestSample>,
    concurrency: usize,
    elapsed: Duration,
) -> MetricsReport {
    let requests = samples.len();
    let success = samples.iter().filter(|sample| sample.ok).count();
    let failed = requests.saturating_sub(success);
    let mut latencies = samples
        .into_iter()
        .map(|sample| sample.elapsed_ms)
        .collect::<Vec<_>>();
    latencies.sort_unstable();
    let elapsed_secs = elapsed.as_secs_f64().max(0.001);
    MetricsReport {
        scenario,
        requests,
        concurrency,
        success,
        failed,
        elapsed_ms: elapsed.as_millis(),
        requests_per_sec: requests as f64 / elapsed_secs,
        min_ms: *latencies.first().unwrap_or(&0),
        p50_ms: percentile(&latencies, 50.0),
        p95_ms: percentile(&latencies, 95.0),
        p99_ms: percentile(&latencies, 99.0),
        max_ms: *latencies.last().unwrap_or(&0),
    }
}

fn percentile(values: &[u128], percentile: f64) -> u128 {
    if values.is_empty() {
        return 0;
    }
    let rank = ((percentile / 100.0) * (values.len().saturating_sub(1) as f64)).ceil() as usize;
    values[rank.min(values.len() - 1)]
}

fn memory_report(pid: u32, samples_kib: &[u64], growth_limit_mb: f64) -> MemoryReport {
    let start = samples_kib.first().copied().unwrap_or(0) as f64 / 1024.0;
    let end = samples_kib.last().copied().unwrap_or(0) as f64 / 1024.0;
    let peak = samples_kib.iter().copied().max().unwrap_or(0) as f64 / 1024.0;
    MemoryReport {
        pid,
        samples: samples_kib.len(),
        start_rss_mb: round2(start),
        end_rss_mb: round2(end),
        peak_rss_mb: round2(peak),
        growth_mb: round2(end - start),
        growth_limit_mb,
    }
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

#[cfg(target_os = "linux")]
fn read_rss_kib(pid: u32) -> Result<u64> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status"))?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            let value = rest
                .split_whitespace()
                .next()
                .ok_or_else(|| anyhow!("VmRSS line is empty"))?;
            return Ok(value.parse()?);
        }
    }
    Err(anyhow!("VmRSS not found for pid {pid}"))
}

#[cfg(all(unix, not(target_os = "linux")))]
fn read_rss_kib(pid: u32) -> Result<u64> {
    let output = Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .context("failed to run ps for RSS sampling")?;
    if !output.status.success() {
        return Err(anyhow!("ps could not read pid {pid}"));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let value = text
        .split_whitespace()
        .next()
        .ok_or_else(|| anyhow!("ps returned no RSS for pid {pid}"))?;
    Ok(value.parse()?)
}

#[cfg(windows)]
fn read_rss_kib(pid: u32) -> Result<u64> {
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &format!("(Get-Process -Id {pid}).WorkingSet64"),
        ])
        .output()
        .context("failed to run powershell for RSS sampling")?;
    if !output.status.success() {
        return Err(anyhow!("powershell could not read pid {pid}"));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let bytes: u64 = text
        .split_whitespace()
        .next()
        .ok_or_else(|| anyhow!("powershell returned no WorkingSet64 for pid {pid}"))?
        .parse()?;
    Ok(bytes / 1024)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_handles_empty_and_bounds() {
        assert_eq!(percentile(&[], 95.0), 0);
        let values = vec![1, 2, 3, 4, 5];
        assert_eq!(percentile(&values, 50.0), 3);
        assert_eq!(percentile(&values, 95.0), 5);
        assert_eq!(percentile(&values, 100.0), 5);
    }

    #[test]
    fn memory_report_uses_end_minus_start_growth() {
        let report = memory_report(42, &[10 * 1024, 12 * 1024, 11 * 1024], 64.0);
        assert_eq!(report.pid, 42);
        assert_eq!(report.start_rss_mb, 10.0);
        assert_eq!(report.end_rss_mb, 11.0);
        assert_eq!(report.peak_rss_mb, 12.0);
        assert_eq!(report.growth_mb, 1.0);
    }

    #[test]
    fn poster_lookup_keeps_only_supplied_lookup_and_locale() {
        let lookup = Lookup {
            offer_id: Some("SKU-1".to_string()),
            product_id: None,
            sku: None,
        };
        let value = poster_lookup_json(&lookup, "studio");
        assert_eq!(value["offer_id"], "SKU-1");
        assert_eq!(value["theme"], "studio");
        assert_eq!(value["locale"], "zh-CN");
        assert!(value.get("product_id").is_none());
    }
}
