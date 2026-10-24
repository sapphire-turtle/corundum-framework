//! Text on the system clipboard, where the window's toolkit cannot read it itself.

/// The primary clip's text, coerced as the system's own paste would, or `None`.
///
/// egui-winit's clipboard covers the desktops; on Android it has none, so a field that
/// takes a long pasted value (an invite ticket) offers a Paste button reading this. `None`
/// where nothing is there, and off Android, where the toolkit's own clipboard is the one
/// to use.
pub fn text() -> Option<String> {
    #[cfg(target_os = "android")]
    {
        read_clipboard()
            .inspect_err(|err| tracing::warn!("could not read the clipboard: {err}"))
            .ok()
            .flatten()
    }
    #[cfg(not(target_os = "android"))]
    {
        None
    }
}

#[cfg(target_os = "android")]
fn read_clipboard() -> jni::errors::Result<Option<String>> {
    use jni::objects::JString;
    use jni::{jni_sig, jni_str};

    crate::android::with_env(|env| {
        let context = crate::android::context(env)?;
        let name = env.new_string("clipboard")?;
        let manager = env
            .call_method(
                &context,
                jni_str!("getSystemService"),
                jni_sig!("(Ljava/lang/String;)Ljava/lang/Object;"),
                &[(&name).into()],
            )?
            .l()?;
        if manager.is_null() {
            return Ok(None);
        }
        let clip = env
            .call_method(
                &manager,
                jni_str!("getPrimaryClip"),
                jni_sig!("()Landroid/content/ClipData;"),
                &[],
            )?
            .l()?;
        if clip.is_null() {
            return Ok(None);
        }
        let count = env
            .call_method(&clip, jni_str!("getItemCount"), jni_sig!("()I"), &[])?
            .i()?;
        if count < 1 {
            return Ok(None);
        }
        let item = env
            .call_method(
                &clip,
                jni_str!("getItemAt"),
                jni_sig!("(I)Landroid/content/ClipData$Item;"),
                &[0.into()],
            )?
            .l()?;
        let text = env
            .call_method(
                &item,
                jni_str!("coerceToText"),
                jni_sig!("(Landroid/content/Context;)Ljava/lang/CharSequence;"),
                &[(&context).into()],
            )?
            .l()?;
        if text.is_null() {
            return Ok(None);
        }
        let text = env
            .call_method(
                &text,
                jni_str!("toString"),
                jni_sig!("()Ljava/lang/String;"),
                &[],
            )?
            .l()?;
        let text = env.cast_local::<JString>(text)?.try_to_string(env)?;
        Ok(Some(text))
    })
}

// Off Android only: on a phone this reads the real clipboard.
#[cfg(all(test, not(target_os = "android")))]
mod tests {
    #[test]
    fn off_android_there_is_no_clipboard() {
        assert_eq!(super::text(), None);
    }
}
