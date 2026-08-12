//! Android storage: public Downloads + All-files access + media scan.
//!
//! GET used to write `/data/user/0/<pkg>/files/Downloads`, which the system
//! Files app cannot see. We land in `/storage/emulated/0/Download` instead.

use std::path::{Path, PathBuf};

use jni::objects::{JObject, JObjectArray, JValue};
use jni::JavaVM;

/// Directory the Files app shows as Downloads.
pub fn public_downloads_dir() -> PathBuf {
    for p in [
        "/storage/emulated/0/Download",
        "/sdcard/Download",
        "/storage/emulated/0/Downloads",
    ] {
        if Path::new(p).is_dir() {
            return PathBuf::from(p);
        }
    }
    PathBuf::from("/storage/emulated/0/Download")
}

/// Open the system "All files access" screen if it isn't already granted.
pub fn ensure_public_downloads() {
    match with_jni(|env, ctx| {
        if is_external_storage_manager(env) {
            log::info!("android: already have MANAGE_EXTERNAL_STORAGE");
            return Ok(());
        }
        log::info!("android: requesting MANAGE_EXTERNAL_STORAGE");
        open_manage_all_files(env, ctx)
    }) {
        Ok(()) => {}
        Err(e) => log::error!("android: storage permission request failed: {e}"),
    }
}

/// Tell MediaStore about a newly written file so Files indexes it immediately.
pub fn scan_file(path: &Path) {
    let Some(path_str) = path.to_str() else { return };
    if let Err(e) = with_jni(|env, ctx| scan_file_jni(env, ctx, path_str)) {
        log::warn!("android: media scan {path_str}: {e}");
    }
}

fn with_jni<T>(
    f: impl FnOnce(&mut jni::JNIEnv, JObject) -> Result<T, String>,
) -> Result<T, String> {
    let ctx = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) }.map_err(|e| e.to_string())?;
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    let context = unsafe { JObject::from_raw(ctx.context() as jni::sys::jobject) };
    f(&mut env, context)
}

fn is_external_storage_manager(env: &mut jni::JNIEnv) -> bool {
    let class = match env.find_class("android/os/Environment") {
        Ok(c) => c,
        Err(_) => return false,
    };
    env.call_static_method(class, "isExternalStorageManager", "()Z", &[])
        .ok()
        .and_then(|v| v.z().ok())
        .unwrap_or(false)
}

fn open_manage_all_files(env: &mut jni::JNIEnv, ctx: JObject) -> Result<(), String> {
    let settings = env
        .find_class("android/provider/Settings")
        .map_err(|e| e.to_string())?;
    let action = env
        .get_static_field(
            settings,
            "ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION",
            "Ljava/lang/String;",
        )
        .map_err(|e| e.to_string())?
        .l()
        .map_err(|e| e.to_string())?;

    let intent_class = env
        .find_class("android/content/Intent")
        .map_err(|e| e.to_string())?;
    let intent = env
        .new_object(
            &intent_class,
            "(Ljava/lang/String;)V",
            &[JValue::Object(&action)],
        )
        .map_err(|e| e.to_string())?;

    let pkg = env
        .call_method(&ctx, "getPackageName", "()Ljava/lang/String;", &[])
        .map_err(|e| e.to_string())?
        .l()
        .map_err(|e| e.to_string())?;
    let prefix = env.new_string("package:").map_err(|e| e.to_string())?;
    let uri_s = env
        .call_method(
            &prefix,
            "concat",
            "(Ljava/lang/String;)Ljava/lang/String;",
            &[JValue::Object(&pkg)],
        )
        .map_err(|e| e.to_string())?
        .l()
        .map_err(|e| e.to_string())?;

    let uri_class = env.find_class("android/net/Uri").map_err(|e| e.to_string())?;
    let uri = env
        .call_static_method(
            uri_class,
            "parse",
            "(Ljava/lang/String;)Landroid/net/Uri;",
            &[JValue::Object(&uri_s)],
        )
        .map_err(|e| e.to_string())?
        .l()
        .map_err(|e| e.to_string())?;

    env.call_method(
        &intent,
        "setData",
        "(Landroid/net/Uri;)Landroid/content/Intent;",
        &[JValue::Object(&uri)],
    )
    .map_err(|e| e.to_string())?;

    let flag = env
        .get_static_field(&intent_class, "FLAG_ACTIVITY_NEW_TASK", "I")
        .map_err(|e| e.to_string())?
        .i()
        .map_err(|e| e.to_string())?;
    env.call_method(&intent, "addFlags", "(I)Landroid/content/Intent;", &[JValue::Int(flag)])
        .map_err(|e| e.to_string())?;

    env.call_method(
        &ctx,
        "startActivity",
        "(Landroid/content/Intent;)V",
        &[JValue::Object(&intent)],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn scan_file_jni(env: &mut jni::JNIEnv, ctx: JObject, path: &str) -> Result<(), String> {
    let jpath = env.new_string(path).map_err(|e| e.to_string())?;
    let str_class = env.find_class("java/lang/String").map_err(|e| e.to_string())?;
    let arr: JObjectArray = env
        .new_object_array(1, str_class, &jpath)
        .map_err(|e| e.to_string())?;
    let scanner = env
        .find_class("android/media/MediaScannerConnection")
        .map_err(|e| e.to_string())?;
    let null = JObject::null();
    env.call_static_method(
        scanner,
        "scanFile",
        "(Landroid/content/Context;[Ljava/lang/String;[Ljava/lang/String;Landroid/media/MediaScannerConnection$OnScanCompletedListener;)V",
        &[
            JValue::Object(&ctx),
            JValue::Object(&arr),
            JValue::Object(&null),
            JValue::Object(&null),
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
