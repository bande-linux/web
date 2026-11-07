use std::time::Instant;

fn rss_kib() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find(|line| line.starts_with("VmRSS:"))
                .and_then(|line| line.split_whitespace().nth(1))
                .and_then(|value| value.parse().ok())
        })
        .unwrap_or(0)
}

fn run(scope: &mut v8::PinScope<'_, '_>, source: &str) -> Option<String> {
    let code = v8::String::new(scope, source)?;
    let script = v8::Script::compile(scope, code, None)?;
    let value = script.run(scope)?;
    Some(value.to_rust_string_lossy(scope))
}

const FIB: &str = "function fib(n){return n<2?n:fib(n-1)+fib(n-2)};fib(35)";
const LOOP: &str = "let s=0;const a=new Float64Array(1024);for(let i=0;i<200000000;i++){a[i&1023]+=i*0.5;s=(s+a[(i*7)&1023])|0}s";
const OBJECTS: &str = "let last;for(let i=0;i<5000000;i++){last={x:i,y:[i],z:'s'+(i&255)}}last.x";
const WASM: &str = "new WebAssembly.Instance(new WebAssembly.Module(new Uint8Array([0,97,115,109,1,0,0,0,1,7,1,96,2,127,127,1,127,3,2,1,0,7,7,1,3,97,100,100,0,0,10,9,1,7,0,32,0,32,1,106,11]))).exports.add(2,40)";

fn main() {
    println!("page_size={}", unsafe { libc::sysconf(libc::_SC_PAGESIZE) });
    println!("v8={}", v8::V8::get_version());
    let rss_start = rss_kib();
    let started = Instant::now();
    let platform = v8::new_default_platform(0, false).make_shared();
    v8::V8::initialize_platform(platform);
    v8::V8::initialize();
    println!("init_us={}", started.elapsed().as_micros());
    println!("rss_after_init_kib={} (start {rss_start})", rss_kib());

    let started = Instant::now();
    let mut isolate = v8::Isolate::new(v8::CreateParams::default());
    println!("isolate_us={}", started.elapsed().as_micros());
    println!("rss_after_isolate_kib={}", rss_kib());
    {
        v8::scope!(let scope, &mut isolate);
        let started = Instant::now();
        let context = v8::Context::new(scope, Default::default());
        println!("context_us={}", started.elapsed().as_micros());
        println!("rss_after_context_kib={}", rss_kib());
        let scope = &mut v8::ContextScope::new(scope, context);
        for (name, source) in [("fib35", FIB), ("float_loop", LOOP), ("objects", OBJECTS), ("wasm", WASM)] {
            let started = Instant::now();
            let result = run(scope, source);
            println!("{name}_ms={} result={result:?}", started.elapsed().as_millis());
        }
        println!("rss_after_work_kib={}", rss_kib());

        let mut contexts = Vec::new();
        let before = rss_kib();
        let started = Instant::now();
        for _ in 0..100 {
            let extra = v8::Context::new(scope, Default::default());
            contexts.push(v8::Global::new(scope, extra));
        }
        println!("context_100_us={} rss_per_context_kib={}", started.elapsed().as_micros(), (rss_kib().saturating_sub(before)) / 100);
    }
    let stats = isolate.get_heap_statistics();
    println!("heap_total={} heap_used={} external={}", stats.total_heap_size(), stats.used_heap_size(), stats.external_memory());
    drop(isolate);
    println!("done");
}
