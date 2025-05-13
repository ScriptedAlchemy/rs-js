use napi::{bindgen_prelude::*, Env, JsObject, Result, JsFunction};
use napi_derive::napi;
use serde::Deserialize;
use std::fs;
use std::path::Path;
use once_cell::sync::Lazy;
use std::time::Instant;
use napi::threadsafe_function::{
    ThreadsafeFunction,
    ErrorStrategy,
    ThreadsafeFunctionCallMode,
    ThreadSafeCallContext,
};

#[derive(Deserialize, Clone, serde::Serialize)]
struct User {
    id: u32,
    #[serde(rename = "firstName")]
    first_name: String,
    age: u32,
    role: String,
}

#[derive(Deserialize)]
struct UserData {
    users: Vec<User>,
}

static USERS: Lazy<Vec<User>> = Lazy::new(|| {
    let path = Path::new("data.json");
    let file_content = fs::read_to_string(path).expect("Failed to read data.json");
    let data: UserData = serde_json::from_str(&file_content).expect("Failed to parse data.json");
    data.users
});

#[napi]
pub fn rust_initialize() -> f64 {
    // Measure initialization time
    let start = Instant::now();
    
    // Force initialization
    let _ = USERS.len();
    
    // Return elapsed time in milliseconds
    let elapsed = start.elapsed();
    let millis = elapsed.as_secs() as f64 * 1000.0 + elapsed.subsec_nanos() as f64 / 1_000_000.0;
    millis
}

#[napi]
pub fn rust_filter(env: Env) -> Result<JsObject> {
    let count = USERS.iter().filter(|u| u.role == "admin").count();
    let mut obj = env.create_object()?;
    obj.set_named_property("count", count as u32)?;
    Ok(obj)
}

struct FilterWithCallbackTask {
    tsfn: ThreadsafeFunction<User, ErrorStrategy::Fatal>,
}

impl Task for FilterWithCallbackTask {
    type Output = u32;
    type JsValue = u32;

    fn compute(&mut self) -> Result<Self::Output> {
        let mut count = 0;
        
        // Process each user sequentially
        for user in USERS.iter() {
            // Create a clone for the callback
            let user_clone = user.clone();
            
            // Call the JS callback
            let should_keep = self.tsfn.call(user_clone, ThreadsafeFunctionCallMode::Blocking) == napi::Status::Ok;
            
            // If the call was successful and the user role is "admin", increment count
            if should_keep && user.role == "admin" {
                count += 1;
            }
        }
        
        Ok(count)
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

#[napi(
    js_name = "filterWithCallback",
    ts_args_type = "callback: (user: {id: number, firstName: string, age: number, role: string}) => boolean",
    ts_return_type = "Promise<number>"
)]
pub fn filter_with_callback(
    _env: Env,
    callback: JsFunction,
) -> Result<AsyncTask<FilterWithCallbackTask>> {
    // Create a threadsafe function
    let tsfn: ThreadsafeFunction<User, ErrorStrategy::Fatal> = callback
        .create_threadsafe_function(
            0, // Max queue size
            |ctx: ThreadSafeCallContext<User>| { // Input: User struct from Rust
                ctx.env.to_js_value(&ctx.value).map(|v| vec![v])
            }
        )?;

    let task = FilterWithCallbackTask { tsfn };
    Ok(AsyncTask::new(task))
}

#[napi(
    js_name = "filterWithCallbackSync",
    ts_args_type = "callback: (user: {id: number, firstName: string, age: number, role: string}) => boolean"
)]
pub fn filter_with_callback_sync(env: Env, callback: JsFunction) -> Result<JsObject> {
    // Create a threadsafe function
    let tsfn: ThreadsafeFunction<User, ErrorStrategy::Fatal> = callback
        .create_threadsafe_function(
            0, // Max queue size
            |ctx: ThreadSafeCallContext<User>| { // Input: User struct from Rust
                ctx.env.to_js_value(&ctx.value).map(|v| vec![v])
            }
        )?;
    
    let mut count = 0;
    
    // Process each user sequentially
    for user in USERS.iter() {
        // Create a clone for the callback
        let user_clone = user.clone();
        
        // Call the JS callback
        let should_keep = tsfn.call(user_clone, ThreadsafeFunctionCallMode::Blocking) == napi::Status::Ok;
        
        // If the call was successful and the user role is "admin", increment count
        if should_keep && user.role == "admin" {
            count += 1;
        }
    }
    
    // Return the count as an object
    let mut obj = env.create_object()?;
    obj.set_named_property("count", count as u32)?;
    Ok(obj)
}
