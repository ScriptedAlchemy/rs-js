const { existsSync, readFileSync } = require('fs')
const { join } = require('path')
const fs = require('fs')

const { platform, arch } = process

let nativeBinding = null
let localFileExisted = false
let loadError = null

function isMusl() {
  // For Node 10
  if (!process.report || typeof process.report.getReport !== 'function') {
    try {
      const lddPath = require('child_process').execSync('which ldd').toString().trim();
      return readFileSync(lddPath, 'utf8').includes('musl')
    } catch (e) {
      return true
    }
  } else {
    const { glibcVersionRuntime } = process.report.getReport().header
    return !glibcVersionRuntime
  }
}

// First, try loading the simple name directly (for local builds without suffix)
try {
  nativeBinding = require('./my-threadsafe-filter.node');
} catch (e) {
  console.error("Failed to load native module directly:", e.message);
  loadError = e;
}

// If direct load fails, throw the error
if (!nativeBinding) {
  if (loadError) {
    throw loadError;
  }
  throw new Error('Failed to load native binding');
}

const { rustFilter, rustInitialize, filterWithCallback, filterWithCallbackSync } = nativeBinding;
console.log("Native module exports:", Object.keys(nativeBinding));

// Our admin filter callback used in the callback-based test
let firstCallDone = false;
const filterAdminCallback = (user) => {
  // Log the first user object to see its structure
  if (!firstCallDone) {
    console.log("First user object received:", JSON.stringify(user));
    firstCallDone = true;
  }
  
  return typeof user.role === 'string' && user.role.toLowerCase() === 'admin';
};

// Benchmark both functions
async function runBenchmark() {
  // Preheat: force data load and parse before timing
  const initTime = rustInitialize();
  console.log(`Data initialization took ${initTime.toFixed(2)}ms (file I/O + JSON parsing)`);

  console.log("\n===== BENCHMARKING RUST FILTERING WITH 50K USERS =====");
  
  // Test 1: Pure Rust in-memory filtering
  const start = performance.now();
  const result = rustFilter();
  const end = performance.now();
  const elapsed = end - start;
  console.log(`Rust filter took ${elapsed.toFixed(2)}ms (in-memory filtering only)`);
  console.log(`Found ${result.count} admin users\n`);
  
  // Test 2: Rust with JS callback filtering (async)
  console.log("TEST 2: Rust with JS callback filtering (async)");
  const callbackStart = performance.now();
  const callbackFilteredCount = await filterWithCallback(filterAdminCallback);
  const callbackEnd = performance.now();
  const callbackTime = callbackEnd - callbackStart;
  console.log(`Async JS callback took ${callbackTime.toFixed(2)}ms`);
  console.log(`Found ${callbackFilteredCount} admin users\n`);
  
  // Test 3: Rust with JS callback filtering (sync)
  console.log("TEST 3: Rust with JS callback filtering (sync)");
  firstCallDone = false; // Reset for new test
  const syncCallbackStart = performance.now();
  const syncCallbackResult = filterWithCallbackSync(filterAdminCallback);
  const syncCallbackEnd = performance.now();
  const syncCallbackTime = syncCallbackEnd - syncCallbackStart;
  console.log(`Sync JS callback took ${syncCallbackTime.toFixed(2)}ms`);
  console.log(`Found ${syncCallbackResult.count} admin users\n`);
  
  // Show comparison between different approaches
  console.log(`Initialization is ${Math.round(initTime/elapsed)}x slower than pure filtering`);
  console.log(`Async JS callback is ${Math.round(callbackTime/elapsed)}x slower than pure filtering`);
  console.log(`Sync JS callback is ${Math.round(syncCallbackTime/elapsed)}x slower than pure filtering`);
  console.log(`Sync/Async JS callback ratio: ${(syncCallbackTime/callbackTime).toFixed(2)}x`);
}

// Execute the benchmark
runBenchmark().catch(err => console.error("Benchmark error:", err));