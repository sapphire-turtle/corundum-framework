package net.sapphire_turtle.corundum_demo;

import android.content.Intent;

import com.google.androidgamesdk.GameActivity;

/**
 * The demo's activity: GameActivity, which loads the library the manifest names
 * (android.app.lib_name) and calls its android_main, and forwards the results of the
 * system's pickers corundum opens (corundum-framework-platform's media::available).
 */
public class MainActivity extends GameActivity {
    @Override
    @SuppressWarnings("deprecation")
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        corundumActivityResult(requestCode, resultCode, data);
    }

    /** Registered by corundum-framework-platform when the window starts. */
    private native void corundumActivityResult(int requestCode, int resultCode, Intent data);
}
