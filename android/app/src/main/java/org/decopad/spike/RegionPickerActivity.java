package org.decopad.spike;

import android.app.Activity;
import android.graphics.Point;
import android.hardware.display.DisplayManager;
import android.os.Bundle;
import android.view.Display;
import android.view.Surface;
import android.view.WindowInsets;
import android.widget.Button;
import android.widget.LinearLayout;
import android.widget.TextView;

import java.util.Locale;

/** Lets the user choose the screen rectangle the tablet maps to. */
public class RegionPickerActivity extends Activity {

    private RegionView regionView;
    private TextView readout;
    private float left;
    private float top;
    private float right;
    private float bottom;
    private float screenWidth = 1080f;
    private float screenHeight = 2400f;

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        AreaMapper.load(this);
        left = AreaMapper.left;
        top = AreaMapper.top;
        right = AreaMapper.right;
        bottom = AreaMapper.bottom;

        DisplayManager displays = getSystemService(DisplayManager.class);
        Display display = displays != null ? displays.getDisplay(Display.DEFAULT_DISPLAY) : null;
        if (display != null) {
            Point size = new Point();
            display.getRealSize(size);
            boolean landscape = display.getRotation() == Surface.ROTATION_90
                    || display.getRotation() == Surface.ROTATION_270;
            int w = size.x;
            int h = size.y;
            if (landscape != (w > h)) {
                int swap = w;
                w = h;
                h = swap;
            }
            screenWidth = w;
            screenHeight = h;
        }

        int pad = dp(16);
        LinearLayout root = new LinearLayout(this);
        root.setOrientation(LinearLayout.VERTICAL);
        root.setPadding(pad, pad, pad, pad);

        TextView title = new TextView(this);
        title.setText("Map the tablet to this part of the screen.\n"
                + "Drag a corner to resize, inside to move, or empty space to draw a new area.");
        title.setTextSize(14);
        root.addView(title);

        readout = new TextView(this);
        readout.setTextSize(13);
        root.addView(readout);

        regionView = new RegionView(this);
        regionView.setAspect(screenWidth / screenHeight);
        regionView.setListener((l, t, r, b) -> {
            left = l;
            top = t;
            right = r;
            bottom = b;
            updateReadout();
        });
        regionView.setRegion(left, top, right, bottom);
        root.addView(regionView, new LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.MATCH_PARENT, 0, 1f));

        LinearLayout presets = new LinearLayout(this);
        presets.setOrientation(LinearLayout.HORIZONTAL);
        presets.addView(button("Full screen", v -> regionView.setRegion(0f, 0f, 1f, 1f)));
        presets.addView(button("Tablet 16:9", v -> {
            float[] region = Mapping.aspectRegion(screenWidth, screenHeight);
            regionView.setRegion(region[0], region[1], region[2], region[3]);
        }));
        root.addView(presets);

        LinearLayout actions = new LinearLayout(this);
        actions.setOrientation(LinearLayout.HORIZONTAL);
        actions.addView(button("Cancel", v -> finish()));
        actions.addView(button("Save", v -> {
            AreaMapper.setRegion(this, left, top, right, bottom);
            finish();
        }));
        root.addView(actions);

        root.setOnApplyWindowInsetsListener((view, insets) -> {
            android.graphics.Insets bars = insets.getInsets(WindowInsets.Type.systemBars());
            view.setPadding(pad, pad + bars.top, pad, pad + bars.bottom);
            return insets;
        });
        setContentView(root);
        updateReadout();
    }

    private Button button(String label, android.view.View.OnClickListener listener) {
        Button button = new Button(this);
        button.setText(label);
        button.setOnClickListener(listener);
        button.setLayoutParams(new LinearLayout.LayoutParams(0,
                LinearLayout.LayoutParams.WRAP_CONTENT, 1f));
        return button;
    }

    private void updateReadout() {
        if (readout != null) {
            readout.setText(String.format(Locale.US,
                    "Left %d%%   Top %d%%   Right %d%%   Bottom %d%%",
                    Math.round(left * 100), Math.round(top * 100),
                    Math.round(right * 100), Math.round(bottom * 100)));
        }
    }

    private int dp(int value) {
        return Math.round(value * getResources().getDisplayMetrics().density);
    }
}
