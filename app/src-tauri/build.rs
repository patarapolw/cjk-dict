/// Settings the app compiles in (read with `option_env!` in src/dicts.rs).
const CONFIG_KEYS: [&str; 6] = [
    "CJK_DICT_R2_ACCOUNT_ID",
    "CJK_DICT_R2_ACCESS_KEY_ID",
    "CJK_DICT_R2_SECRET_ACCESS_KEY",
    "CJK_DICT_R2_BUCKET",
    "CJK_DICT_R2_PREFIX",
    "CJK_DICT_BASE_URL",
];

fn main() {
    // Cargo doesn't read .env files: pass CONFIG_KEYS from the repo-root .env to the
    // compiler. Variables already set in the environment win.
    let env_file = "../../.env";
    println!("cargo:rerun-if-changed={env_file}");
    for key in CONFIG_KEYS {
        println!("cargo:rerun-if-env-changed={key}");
    }
    if let Ok(vars) = dotenvy::from_path_iter(env_file) {
        for (key, value) in vars.flatten() {
            if CONFIG_KEYS.contains(&key.as_str()) && std::env::var_os(&key).is_none() {
                println!("cargo:rustc-env={key}={value}");
            }
        }
    }

    tauri_build::build()
}
