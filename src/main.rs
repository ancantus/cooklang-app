pub mod repository;
mod ui;

fn main() {
    #[cfg(target_os = "android")]
    init_rustls_platform_verifier();
    dioxus::launch(ui::App);
}

/// Initialize rustls' Android platform verifier.
///
/// The gix HTTP transport uses reqwest with rustls, whose default certificate
/// verifier on Android looks up the system trust store through JNI. That
/// lookup panics unless the verifier is initialized with the app context
/// before the first HTTPS request, so do it once at startup.
#[cfg(target_os = "android")]
fn init_rustls_platform_verifier() {
    use jni::{objects::JObject, JavaVM};

    let android = ndk_context::android_context();

    let vm = unsafe { JavaVM::from_raw(android.vm().cast()) };

    let result: Result<(), jni::errors::Error> = vm.attach_current_thread(|env| {
        // The context pointer from the Android glue outlives this scope; the
        // verifier stores its own global reference before returning.
        let context = unsafe { JObject::from_raw(env, android.context().cast::<jni::sys::_jobject>()) };
        rustls_platform_verifier::android::init_with_env(env, context)?;
        Ok(())
    });

    if let Err(error) = result {
        eprintln!("TLS platform verifier initialization failed: {error}");
    }
}
