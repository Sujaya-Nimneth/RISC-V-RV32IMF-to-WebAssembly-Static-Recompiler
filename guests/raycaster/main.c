#include "rv_abi.h"
#include <stdint.h>

int printf(const char *fmt, ...);

#define W 320
#define H 200
#define MAP_W 12
#define MAP_H 12

static const uint8_t world[MAP_H][MAP_W] = {
    {1,1,1,1,1,1,1,1,1,1,1,1},
    {1,0,0,0,0,0,0,0,0,0,0,1},
    {1,0,2,2,0,0,0,3,3,0,0,1},
    {1,0,2,0,0,0,0,0,3,0,0,1},
    {1,0,0,0,0,4,4,0,0,0,0,1},
    {1,0,0,0,0,4,0,0,0,2,0,1},
    {1,0,3,0,0,0,0,0,0,2,0,1},
    {1,0,3,0,0,0,0,4,0,0,0,1},
    {1,0,0,0,2,0,0,4,0,0,0,1},
    {1,0,0,0,2,0,0,0,0,3,0,1},
    {1,0,0,0,0,0,0,0,0,0,0,1},
    {1,1,1,1,1,1,1,1,1,1,1,1},
};

static inline float f_abs(float x) { return x < 0.0f ? -x : x; }

static float f_sin(float x) {
    const float PI = 3.14159265f;
    while (x > PI) x -= 2.0f * PI;
    while (x < -PI) x += 2.0f * PI;
    float x2 = x * x;
    return x * (1.0f - x2 * (0.16666667f - x2 * (0.00833333f - x2 * 0.00019841f)));
}

static float f_cos(float x) {
    return f_sin(x + 1.57079633f);
}

static inline uint32_t rgba(uint8_t r, uint8_t g, uint8_t b) {
    return (uint32_t)r | ((uint32_t)g << 8) | ((uint32_t)b << 16) | 0xFF000000u;
}

int main(void) {
    float px = 2.5f, py = 2.5f;
    float dir_angle = 0.4f;
    volatile uint32_t *fb = RV_FB_BASE;
    int last_mx = 160;
    int has_mouse = 0;

    printf("3D Raycaster started (WASD/Arrows to move, Shift to sprint)\n");

    for (;;) {
        uint32_t keys = rv_poll_input();
        int32_t mx = (int32_t)RV_MMIO_BASE[RV_MMIO_MOUSE_X];
        uint32_t mbtn = RV_MMIO_BASE[RV_MMIO_MOUSE_BTN];
        if ((mbtn & 1u) != 0) {
            if (has_mouse) {
                dir_angle += (float)(mx - last_mx) * 0.01f;
            }
            last_mx = mx;
            has_mouse = 1;
        } else {
            has_mouse = 0;
        }

        float speed = (keys & RV_KEY_SHIFT) ? 0.09f : 0.05f;
        float rot = 0.045f;
        if (keys & (RV_KEY_LEFT))  dir_angle -= rot;
        if (keys & (RV_KEY_RIGHT)) dir_angle += rot;
        if (keys == 0) dir_angle += 0.008f; // Gentle idle orbit

        float dx = f_cos(dir_angle);
        float dy = f_sin(dir_angle);
        float nx = px, ny = py;

        if (keys & (RV_KEY_W | RV_KEY_UP))   { nx += dx * speed; ny += dy * speed; }
        if (keys & (RV_KEY_S | RV_KEY_DOWN)) { nx -= dx * speed; ny -= dy * speed; }
        if (keys & RV_KEY_A) { nx += dy * speed; ny -= dx * speed; }
        if (keys & RV_KEY_D) { nx -= dy * speed; ny += dx * speed; }

        if (world[(int)py][(int)nx] == 0) px = nx;
        if (world[(int)ny][(int)px] == 0) py = ny;

        float plane_x = -dy * 0.66f;
        float plane_y = dx * 0.66f;

        for (int x = 0; x < W; x++) {
            float cam_x = 2.0f * (float)x / (float)W - 1.0f;
            float ray_dx = dx + plane_x * cam_x;
            float ray_dy = dy + plane_y * cam_x;

            int map_x = (int)px;
            int map_y = (int)py;
            float delta_x = (ray_dx == 0.0f) ? 1e30f : f_abs(1.0f / ray_dx);
            float delta_y = (ray_dy == 0.0f) ? 1e30f : f_abs(1.0f / ray_dy);

            int step_x, step_y;
            float side_x, side_y;
            if (ray_dx < 0.0f) { step_x = -1; side_x = (px - (float)map_x) * delta_x; }
            else               { step_x =  1; side_x = ((float)map_x + 1.0f - px) * delta_x; }
            if (ray_dy < 0.0f) { step_y = -1; side_y = (py - (float)map_y) * delta_y; }
            else               { step_y =  1; side_y = ((float)map_y + 1.0f - py) * delta_y; }

            int hit = 0, side = 0;
            while (hit == 0) {
                if (side_x < side_y) { side_x += delta_x; map_x += step_x; side = 0; }
                else                 { side_y += delta_y; map_y += step_y; side = 1; }
                if (map_x < 0 || map_x >= MAP_W || map_y < 0 || map_y >= MAP_H) { hit = 1; break; }
                hit = world[map_y][map_x];
            }

            float perp = (side == 0) ? (side_x - delta_x) : (side_y - delta_y);
            if (perp < 0.05f) perp = 0.05f;
            int line_h = (int)((float)H / perp);
            int draw_s = -line_h / 2 + H / 2;
            int draw_e =  line_h / 2 + H / 2;
            if (draw_s < 0) draw_s = 0;
            if (draw_e >= H) draw_e = H - 1;

            float wall_x = (side == 0) ? (py + perp * ray_dy) : (px + perp * ray_dx);
            wall_x -= (float)(int)wall_x;
            int tex_x = (int)(wall_x * 16.0f) & 15;

            float shade = 1.0f / (1.0f + perp * 0.25f);
            if (side == 1) shade *= 0.72f;

            uint8_t br = 60, bg = 140, bb = 220;
            if (hit == 2) { br = 220; bg = 80;  bb = 70; }
            if (hit == 3) { br = 70;  bg = 200; bb = 110; }
            if (hit == 4) { br = 220; bg = 180; bb = 70; }

            for (int y = 0; y < draw_s; y++) {
                uint8_t sky = (uint8_t)(20 + (y * 30) / H);
                fb[y * W + x] = rgba(sky, sky + 8, sky + 24);
            }
            for (int y = draw_s; y <= draw_e; y++) {
                int d = (y - H / 2 + line_h / 2) * 16 / (line_h > 0 ? line_h : 1);
                int checker = ((tex_x ^ d) & 4) ? 255 : 190;
                uint8_t r = (uint8_t)((float)br * shade * (float)checker / 255.0f);
                uint8_t g = (uint8_t)((float)bg * shade * (float)checker / 255.0f);
                uint8_t b = (uint8_t)((float)bb * shade * (float)checker / 255.0f);
                fb[y * W + x] = rgba(r, g, b);
            }
            for (int y = draw_e + 1; y < H; y++) {
                uint8_t fl = (uint8_t)(16 + ((y - H / 2) * 50) / (H / 2));
                fb[y * W + x] = rgba(fl, fl + 4, fl + 6);
            }
        }

        // Minimap overlay (top-left)
        for (int my = 0; my < MAP_H; my++) {
            for (int mx_i = 0; mx_i < MAP_W; mx_i++) {
                uint32_t c = world[my][mx_i] ? rgba(180, 200, 220) : rgba(15, 20, 30);
                for (int sy = 0; sy < 3; sy++)
                    for (int sx = 0; sx < 3; sx++)
                        fb[(my * 3 + 4 + sy) * W + (mx_i * 3 + 4 + sx)] = c;
            }
        }
        fb[((int)(py * 3.0f) + 4) * W + ((int)(px * 3.0f) + 4)] = rgba(255, 60, 60);

        rv_present_frame(W, H);
    }
    return 0;
}
