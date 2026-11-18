//! Ponte JNI para `com.academialuis.workout.MainActivity` (android/java).

use super::{FileResult, Platform};
use jni::objects::{JObject, JString, JValue};
use jni::{JNIEnv, JavaVM};
use slint::android::AndroidApp;
use std::path::{Path, PathBuf};

pub struct AndroidPlatform {
    app: AndroidApp,
    data_dir: PathBuf,
}

impl AndroidPlatform {
    pub fn new(app: AndroidApp) -> AndroidPlatform {
        let data_dir = app.internal_data_path().unwrap_or_else(|| PathBuf::from("/data/local/tmp/workout"));
        AndroidPlatform { app, data_dir }
    }

    fn with_env<R>(&self, f: impl FnOnce(&mut JNIEnv, &JObject) -> jni::errors::Result<R>) -> Option<R> {
        // SAFETY: ponteiros fornecidos pelo android-activity, válidos durante a vida do app.
        let vm = unsafe { JavaVM::from_raw(self.app.vm_as_ptr() as *mut jni::sys::JavaVM) }.ok()?;
        let mut env = vm.attach_current_thread().ok()?;
        let activity = unsafe { JObject::from_raw(self.app.activity_as_ptr() as jni::sys::jobject) };
        let r = f(&mut env, &activity);
        if env.exception_check().unwrap_or(false) {
            let _ = env.exception_describe();
            let _ = env.exception_clear();
        }
        match r {
            Ok(v) => Some(v),
            Err(e) => {
                log::error!("JNI: {e}");
                None
            }
        }
    }

    fn call_void(&self, name: &str, sig: &str, args: impl for<'e> FnOnce(&mut JNIEnv<'e>) -> jni::errors::Result<Vec<OwnedArg<'e>>>) {
        self.with_env(|env, act| {
            let owned = args(env)?;
            let vals: Vec<JValue> = owned.iter().map(|a| a.as_jvalue()).collect();
            env.call_method(act, name, sig, &vals)?;
            Ok(())
        });
    }

    fn call_string(&self, name: &str) -> Option<String> {
        self.with_env(|env, act| {
            let o = env.call_method(act, name, "()Ljava/lang/String;", &[])?.l()?;
            if o.is_null() {
                return Ok(None);
            }
            let s: String = env.get_string(&JString::from(o))?.into();
            Ok(Some(s))
        })
        .flatten()
    }
}

enum OwnedArg<'a> {
    Obj(JObject<'a>),
    Int(i32),
    Long(i64),
    Bool(bool),
}

impl<'a> OwnedArg<'a> {
    fn as_jvalue(&self) -> JValue<'_, 'a> {
        match self {
            OwnedArg::Obj(o) => JValue::Object(o),
            OwnedArg::Int(i) => JValue::Int(*i),
            OwnedArg::Long(l) => JValue::Long(*l),
            OwnedArg::Bool(b) => JValue::Bool(*b as u8),
        }
    }
}

fn jstr<'a>(env: &mut JNIEnv<'a>, s: &str) -> jni::errors::Result<OwnedArg<'a>> {
    Ok(OwnedArg::Obj(JObject::from(env.new_string(s)?)))
}

impl Platform for AndroidPlatform {
    fn is_android(&self) -> bool {
        true
    }

    fn data_dir(&self) -> PathBuf {
        self.data_dir.clone()
    }

    fn app_version(&self) -> String {
        self.call_string("appVersion").unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string())
    }

    fn open_document(&self, mimes: &[&str]) {
        let csv = mimes.join(",");
        self.call_void("openDocument", "(Ljava/lang/String;)V", |env| Ok(vec![jstr(env, &csv)?]));
    }

    fn save_document(&self, suggested_name: &str, mime: &str, src: &Path) {
        let src = src.to_string_lossy().to_string();
        self.call_void(
            "createDocument",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)V",
            |env| Ok(vec![jstr(env, suggested_name)?, jstr(env, mime)?, jstr(env, &src)?]),
        );
    }

    fn poll_file_result(&self) -> Option<FileResult> {
        let s = self.call_string("pollFileResult")?;
        let parts: Vec<&str> = s.splitn(4, '|').collect();
        let status = parts.get(1).copied().unwrap_or("");
        Some(match status {
            "ok" if parts[0] == "open" => FileResult::Opened {
                path: PathBuf::from(parts.get(2).copied().unwrap_or("")),
                name: parts.get(3).copied().unwrap_or("").to_string(),
            },
            "ok" => FileResult::Saved { name: parts.get(3).copied().unwrap_or("").to_string() },
            "cancel" => FileResult::Cancelled,
            _ => FileResult::Error(parts.get(2).copied().unwrap_or("Erro desconhecido.").to_string()),
        })
    }

    fn notifications_allowed(&self) -> bool {
        self.with_env(|env, act| env.call_method(act, "notificationsAllowed", "()Z", &[])?.z()).unwrap_or(false)
    }

    fn request_notification_permission(&self) {
        self.call_void("requestNotificationPermission", "()V", |_| Ok(vec![]));
    }

    fn poll_permission_result(&self) -> Option<bool> {
        let r = self.with_env(|env, act| env.call_method(act, "pollPermissionResult", "()I", &[])?.i())?;
        match r {
            1 => Some(true),
            0 => Some(false),
            _ => None,
        }
    }

    fn open_notification_settings(&self) {
        self.call_void("openNotificationSettings", "()V", |_| Ok(vec![]));
    }

    fn schedule_alarm(&self, id: i32, at_boot_ms: i64, title: &str, text: &str, sound: bool, vibrate: bool) {
        self.call_void(
            "scheduleAlarm",
            "(IJLjava/lang/String;Ljava/lang/String;ZZ)V",
            |env| {
                Ok(vec![
                    OwnedArg::Int(id),
                    OwnedArg::Long(at_boot_ms),
                    jstr(env, title)?,
                    jstr(env, text)?,
                    OwnedArg::Bool(sound),
                    OwnedArg::Bool(vibrate),
                ])
            },
        );
    }

    fn cancel_alarm(&self, id: i32) {
        self.call_void("cancelAlarm", "(I)V", |_| Ok(vec![OwnedArg::Int(id)]));
    }

    fn show_countdown(&self, end_wall_ms: i64, title: &str, text: &str) {
        self.call_void("showCountdown", "(JLjava/lang/String;Ljava/lang/String;)V", |env| {
            Ok(vec![OwnedArg::Long(end_wall_ms), jstr(env, title)?, jstr(env, text)?])
        });
    }

    fn cancel_countdown(&self) {
        self.call_void("cancelCountdown", "()V", |_| Ok(vec![]));
    }

    fn play_tone(&self, tone: i32, duration_ms: i32) {
        self.call_void("playTone", "(II)V", |_| Ok(vec![OwnedArg::Int(tone), OwnedArg::Int(duration_ms)]));
    }

    fn vibrate(&self, ms: i64) {
        self.call_void("vibrate", "(J)V", |_| Ok(vec![OwnedArg::Long(ms)]));
    }

    fn keep_screen_on(&self, on: bool) {
        self.call_void("keepScreenOn", "(Z)V", |_| Ok(vec![OwnedArg::Bool(on)]));
    }

    fn move_to_back(&self) {
        self.call_void("moveToBack", "()V", |_| Ok(vec![]));
    }
}
