//! The margins a window drawn edge to edge must leave for the system's bars, the display
//! cutout and the keyboard.

/// The margins in physical pixels — left, top, right, bottom — or `None` where the platform
/// does not report them.
///
/// Android reads them from the activity's window, so they are there once the window has
/// told [`android::set_activity`](crate::android::set_activity) which activity it is. Off
/// Android, `None`: egui-winit hands iOS's safe area to egui itself, and the desktops have
/// none.
pub fn window_insets() -> Option<[i32; 4]> {
    #[cfg(target_os = "android")]
    {
        read_insets()
            .inspect_err(|err| tracing::debug!("could not read the window insets: {err}"))
            .ok()
            .flatten()
    }
    #[cfg(not(target_os = "android"))]
    {
        None
    }
}

#[cfg(target_os = "android")]
fn read_insets() -> jni::errors::Result<Option<[i32; 4]>> {
    use jni::{jni_sig, jni_str};

    crate::android::with_env(|env| {
        let Some(activity) = crate::android::activity(env) else {
            return Ok(None);
        };
        let window = env
            .call_method(
                &activity,
                jni_str!("getWindow"),
                jni_sig!("()Landroid/view/Window;"),
                &[],
            )?
            .l()?;
        let decor = env
            .call_method(
                &window,
                jni_str!("getDecorView"),
                jni_sig!("()Landroid/view/View;"),
                &[],
            )?
            .l()?;
        let insets = env
            .call_method(
                &decor,
                jni_str!("getRootWindowInsets"),
                jni_sig!("()Landroid/view/WindowInsets;"),
                &[],
            )?
            .l()?;
        if insets.is_null() {
            return Ok(None);
        }
        let sdk = env
            .get_static_field(
                jni_str!("android/os/Build$VERSION"),
                jni_str!("SDK_INT"),
                jni_sig!("I"),
            )?
            .i()?;
        if sdk >= 30 {
            // WindowInsets.Type.systemBars() | ime() | displayCutout()
            let mut kind = |name| {
                env.call_static_method(
                    jni_str!("android/view/WindowInsets$Type"),
                    name,
                    jni_sig!("()I"),
                    &[],
                )
                .and_then(|v| v.i())
            };
            let kinds = kind(jni_str!("systemBars"))?
                | kind(jni_str!("ime"))?
                | kind(jni_str!("displayCutout"))?;
            let rect = env
                .call_method(
                    &insets,
                    jni_str!("getInsets"),
                    jni_sig!("(I)Landroid/graphics/Insets;"),
                    &[kinds.into()],
                )?
                .l()?;
            let mut side = |name| {
                env.get_field(&rect, name, jni_sig!("I"))
                    .and_then(|v| v.i())
            };
            Ok(Some([
                side(jni_str!("left"))?,
                side(jni_str!("top"))?,
                side(jni_str!("right"))?,
                side(jni_str!("bottom"))?,
            ]))
        } else {
            let mut side = |name| {
                env.call_method(&insets, name, jni_sig!("()I"), &[])
                    .and_then(|v| v.i())
            };
            Ok(Some([
                side(jni_str!("getSystemWindowInsetLeft"))?,
                side(jni_str!("getSystemWindowInsetTop"))?,
                side(jni_str!("getSystemWindowInsetRight"))?,
                side(jni_str!("getSystemWindowInsetBottom"))?,
            ]))
        }
    })
}

// On Android too: the insets come from a window's activity, and a test has none.
#[cfg(test)]
mod tests {
    #[test]
    fn without_an_activity_there_are_no_insets() {
        assert_eq!(super::window_insets(), None);
    }
}
