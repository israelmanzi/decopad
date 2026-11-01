package org.decopad.spike;

import android.content.Context;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Paint;
import android.graphics.RectF;
import android.view.MotionEvent;
import android.view.View;

/**
 * A scaled phone outline with a draggable region. Drag a corner to resize,
 * drag inside to move, or drag in empty space to draw a new rectangle.
 */
final class RegionView extends View {

    interface Listener {
        void onChange(float left, float top, float right, float bottom);
    }

    private static final float MIN_SIZE = 0.05f;

    private final float density;
    private final float handleRadius;

    private final Paint screenPaint = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Paint outlinePaint = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Paint gridPaint = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Paint regionFill = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Paint regionBorder = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Paint handlePaint = new Paint(Paint.ANTI_ALIAS_FLAG);

    private final RectF screen = new RectF();

    private float aspect = 1080f / 2400f;
    private float left;
    private float top;
    private float right = 1f;
    private float bottom = 1f;

    private Listener listener;
    private int draggingHandle = -1;
    private boolean moving;
    private boolean rubberBand;
    private float anchorU;
    private float anchorV;
    private float startU;
    private float startV;
    private float moveLeft;
    private float moveTop;
    private float moveRight;
    private float moveBottom;

    RegionView(Context context) {
        this(context, null);
    }

    RegionView(Context context, android.util.AttributeSet attrs) {
        super(context, attrs);
        density = getResources().getDisplayMetrics().density;
        handleRadius = 11 * density;

        screenPaint.setColor(Color.rgb(24, 26, 32));
        outlinePaint.setColor(Color.rgb(90, 95, 110));
        outlinePaint.setStyle(Paint.Style.STROKE);
        outlinePaint.setStrokeWidth(1.5f * density);
        gridPaint.setColor(Color.argb(36, 255, 255, 255));
        gridPaint.setStrokeWidth(density);
        regionFill.setColor(Color.argb(64, 90, 170, 255));
        regionBorder.setColor(Color.rgb(90, 170, 255));
        regionBorder.setStyle(Paint.Style.STROKE);
        regionBorder.setStrokeWidth(2.5f * density);
        handlePaint.setColor(Color.WHITE);
    }

    void setAspect(float aspect) {
        if (aspect > 0f) {
            this.aspect = aspect;
            invalidate();
        }
    }

    void setRegion(float newLeft, float newTop, float newRight, float newBottom) {
        left = clamp01(newLeft);
        top = clamp01(newTop);
        right = clamp01(newRight);
        bottom = clamp01(newBottom);
        if (right - left < MIN_SIZE) right = Math.min(1f, left + MIN_SIZE);
        if (bottom - top < MIN_SIZE) bottom = Math.min(1f, top + MIN_SIZE);
        invalidate();
        notifyListener();
    }

    void setListener(Listener listener) {
        this.listener = listener;
    }

    @Override
    protected void onSizeChanged(int w, int h, int oldw, int oldh) {
        super.onSizeChanged(w, h, oldw, oldh);
        float pad = 16 * density;
        float availableWidth = w - 2 * pad;
        float availableHeight = h - 2 * pad;
        float width = availableWidth;
        float height = width / aspect;
        if (height > availableHeight) {
            height = availableHeight;
            width = height * aspect;
        }
        float cx = w / 2f;
        float cy = h / 2f;
        screen.set(cx - width / 2f, cy - height / 2f, cx + width / 2f, cy + height / 2f);
    }

    private RectF regionRect() {
        return new RectF(
                screen.left + left * screen.width(),
                screen.top + top * screen.height(),
                screen.left + right * screen.width(),
                screen.top + bottom * screen.height());
    }

    @Override
    protected void onDraw(Canvas canvas) {
        float radius = 10 * density;
        canvas.drawRoundRect(screen, radius, radius, screenPaint);
        canvas.drawRoundRect(screen, radius, radius, outlinePaint);

        for (int i = 1; i <= 3; i++) {
            float x = screen.left + screen.width() * i / 4f;
            float y = screen.top + screen.height() * i / 4f;
            canvas.drawLine(x, screen.top, x, screen.bottom, gridPaint);
            canvas.drawLine(screen.left, y, screen.right, y, gridPaint);
        }

        RectF region = regionRect();
        canvas.drawRect(region, regionFill);
        canvas.drawRect(region, regionBorder);
        for (float x : new float[]{region.left, region.right}) {
            for (float y : new float[]{region.top, region.bottom}) {
                canvas.drawCircle(x, y, handleRadius, handlePaint);
            }
        }
    }

    @Override
    public boolean onTouchEvent(MotionEvent event) {
        float x = event.getX();
        float y = event.getY();
        switch (event.getActionMasked()) {
            case MotionEvent.ACTION_DOWN: {
                draggingHandle = hitHandle(x, y);
                if (draggingHandle >= 0) {
                    moving = false;
                    rubberBand = false;
                } else if (regionRect().contains(x, y)) {
                    moving = true;
                    rubberBand = false;
                    startU = u(x);
                    startV = v(y);
                    moveLeft = left;
                    moveTop = top;
                    moveRight = right;
                    moveBottom = bottom;
                } else {
                    rubberBand = true;
                    moving = false;
                    anchorU = clamp01(u(x));
                    anchorV = clamp01(v(y));
                    left = right = anchorU;
                    top = bottom = anchorV;
                    notifyListener();
                }
                invalidate();
                return true;
            }
            case MotionEvent.ACTION_MOVE: {
                float cu = clamp01(u(x));
                float cv = clamp01(v(y));
                if (draggingHandle >= 0) {
                    resize(cu, cv);
                } else if (moving) {
                    move(cu, cv);
                } else if (rubberBand) {
                    left = Math.min(anchorU, cu);
                    right = Math.max(anchorU, cu);
                    top = Math.min(anchorV, cv);
                    bottom = Math.max(anchorV, cv);
                }
                notifyListener();
                invalidate();
                return true;
            }
            case MotionEvent.ACTION_UP:
            case MotionEvent.ACTION_CANCEL: {
                if (rubberBand) {
                    if (right - left < MIN_SIZE) right = Math.min(1f, left + MIN_SIZE);
                    if (bottom - top < MIN_SIZE) bottom = Math.min(1f, top + MIN_SIZE);
                }
                draggingHandle = -1;
                moving = false;
                rubberBand = false;
                notifyListener();
                invalidate();
                return true;
            }
            default:
                return super.onTouchEvent(event);
        }
    }

    private void resize(float cu, float cv) {
        switch (draggingHandle) {
            case 0: // top left
                left = Math.min(cu, right - MIN_SIZE);
                top = Math.min(cv, bottom - MIN_SIZE);
                break;
            case 1: // top right
                right = Math.max(cu, left + MIN_SIZE);
                top = Math.min(cv, bottom - MIN_SIZE);
                break;
            case 2: // bottom left
                left = Math.min(cu, right - MIN_SIZE);
                bottom = Math.max(cv, top + MIN_SIZE);
                break;
            case 3: // bottom right
                right = Math.max(cu, left + MIN_SIZE);
                bottom = Math.max(cv, top + MIN_SIZE);
                break;
            default:
                break;
        }
        left = clamp01(left);
        top = clamp01(top);
        right = clamp01(right);
        bottom = clamp01(bottom);
    }

    private void move(float cu, float cv) {
        float width = moveRight - moveLeft;
        float height = moveBottom - moveTop;
        float newLeft = clamp(moveLeft + (cu - startU), 0f, 1f - width);
        float newTop = clamp(moveTop + (cv - startV), 0f, 1f - height);
        left = newLeft;
        right = newLeft + width;
        top = newTop;
        bottom = newTop + height;
    }

    private int hitHandle(float x, float y) {
        RectF region = regionRect();
        float[][] corners = {
                {region.left, region.top},
                {region.right, region.top},
                {region.left, region.bottom},
                {region.right, region.bottom}
        };
        float threshold = handleRadius * 1.8f;
        for (int i = 0; i < corners.length; i++) {
            float dx = x - corners[i][0];
            float dy = y - corners[i][1];
            if (dx * dx + dy * dy <= threshold * threshold) return i;
        }
        return -1;
    }

    private float u(float x) {
        return (x - screen.left) / screen.width();
    }

    private float v(float y) {
        return (y - screen.top) / screen.height();
    }

    private void notifyListener() {
        if (listener != null) listener.onChange(left, top, right, bottom);
    }

    private static float clamp01(float value) {
        return clamp(value, 0f, 1f);
    }

    private static float clamp(float value, float min, float max) {
        return Math.max(min, Math.min(max, value));
    }
}
