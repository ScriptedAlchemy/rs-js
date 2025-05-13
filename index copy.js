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

// Get the functions we need from the native module
const { filterUsersWithCallbackFromFile, filterAdminsSimple, filterAdminsSync } = nativeBinding;

console.log("Native module exports:", Object.keys(nativeBinding));

// Our admin filter callback used in the callback-based test
const filterAdminCallback = (user) => {
  return typeof user.role === 'string' && user.role.toLowerCase() === 'admin';
};

// Benchmark both functions
async function runBenchmark() {
  console.log("\n===== BENCHMARKING RUST FILTERING WITH 50K USERS =====");
  
  // Test 1: Simple Pure Rust filtering with direct par_iter().filter() approach
  console.log("TEST 1: Pure Rust filtering with simple approach (filterAdminsSimple)");
  const simpleStart = performance.now();
  const simpleFilteredCount = await filterAdminsSimple();
  const simpleEnd = performance.now();
  const simpleTime = simpleEnd - simpleStart;
  console.log(`Pure Rust simple filtering took ${simpleTime.toFixed(2)}ms`);
  console.log(`Found ${simpleFilteredCount} admin users\n`);
  
  // Test 2: Rust with JS callback filtering
  console.log("TEST 2: Rust with JS callback filtering");
  const callbackStart = performance.now();
  const callbackFilteredCount = await filterUsersWithCallbackFromFile(filterAdminCallback);
  const callbackEnd = performance.now();
  const callbackTime = callbackEnd - callbackStart;
  console.log(`Rust with JS callback took ${callbackTime.toFixed(2)}ms`);
  console.log(`Found ${callbackFilteredCount} admin users\n`);
  
  // Test 3: Synchronous Rust filtering (no Tasks, simplest approach)
  console.log("TEST 3: Synchronous Rust filtering (filterAdminsSync)");
  const syncStart = performance.now();
  const syncFilteredAdmins = filterAdminsSync(); // Direct synchronous call
  const syncEnd = performance.now();
  const syncTime = syncEnd - syncStart;
  console.log(`Synchronous Rust filtering took ${syncTime.toFixed(2)}ms`);
  console.log(`Found ${syncFilteredAdmins.count} admin users\n`);
  
  // Comparison
  console.log("===== BENCHMARK RESULTS =====");
  console.log(`Simple Rust implementation (par_iter): ${simpleTime.toFixed(2)}ms`);
  console.log(`JS Callback implementation: ${callbackTime.toFixed(2)}ms`);
  console.log(`Synchronous Rust implementation: ${syncTime.toFixed(2)}ms`);
  
  // Find the fastest method
  const times = [
    { name: "Simple par_iter", time: simpleTime },
    { name: "JS Callback", time: callbackTime },
    { name: "Sync Direct", time: syncTime }
  ];
  
  times.sort((a, b) => a.time - b.time);
  console.log(`\n🏆 FASTEST: ${times[0].name} (${times[0].time.toFixed(2)}ms)`);
  
  // Compare each to the fastest
  for (let i = 1; i < times.length; i++) {
    const diff = times[i].time - times[0].time;
    const percent = (diff / times[0].time * 100).toFixed(1);
    console.log(`${times[i].name} is ${diff.toFixed(2)}ms slower (${percent}% slower)`);
  }
  
  // Results
  if (simpleFilteredCount === callbackFilteredCount &&
      simpleFilteredCount === syncFilteredAdmins.count) {
    console.log(`\n✓ All three methods found the same number of admin users: ${simpleFilteredCount}`);
  } else {
    console.log(`\n⚠ Different results: ` +
      `Simple: ${simpleFilteredCount}, ` +
      `JS callback: ${callbackFilteredCount}, ` +
      `Sync: ${syncFilteredAdmins.count}`);
  }
}

// Execute the benchmark
runBenchmark().catch(err => console.error("Benchmark error:", err));