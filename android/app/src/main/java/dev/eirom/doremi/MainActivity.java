package dev.eirom.doremi;

import android.os.Bundle;
import com.google.androidgamesdk.GameActivity;

/** Only Android lifecycle and window integration. Domain state lives in Rust. */
public final class MainActivity extends GameActivity {
    static { System.loadLibrary("doremi_app"); }
    @Override public void onCreate(Bundle state) {
        super.onCreate(state);
        // Keep content clear of system bars, including target SDK 36 edge-to-edge.
        android.view.View content = findViewById(android.R.id.content);
        content.setOnApplyWindowInsetsListener((view, insets) -> {
            view.setPadding(insets.getSystemWindowInsetLeft(), insets.getSystemWindowInsetTop(),
                insets.getSystemWindowInsetRight(), insets.getSystemWindowInsetBottom());
            return insets;
        });
        content.requestApplyInsets();
    }
}
