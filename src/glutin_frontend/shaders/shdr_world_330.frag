#version 330 core

// general parameters
const int WIDTH = {{WIDTH_TILES}};
const int HEIGHT = {{HEIGHT_TILES}};

// shader-specific parameters
const int NUM_TILE_CHANNELS = {{NUM_TILE_CHANNELS}};
const int TILE_STATUS_IDX = {{TILE_STATUS_IDX}};
const int TILE_STATUS_VISIBLE = {{TILE_STATUS_VISIBLE}};
const int STATUS_BITS_PER_CHANNEL = {{STATUS_BITS_PER_CHANNEL}};
const int CHANNEL_PRESENT_OFFSET = {{CHANNEL_PRESENT_OFFSET}};
const int CHANNEL_DIMINISH_OFFSET = {{CHANNEL_DIMINISH_OFFSET}};

in vec2 v_CellPos;

out vec4 Target0;

uniform sampler2D t_Texture;

uniform b_TileMapInfo {
    vec2 u_TexRatio;
    vec2 u_Centre;
};

struct TileMapData {
    vec4 data;
};

uniform b_TileMap {
    TileMapData u_Data[WIDTH * HEIGHT];
};

vec4 blend(vec4 current, vec4 new) {
    vec3 delta = vec3(new - current);
    vec3 result = vec3(current) + delta * new[3];
    return vec4(result, max(current[3], new[3]));
}

const float DIM_COEF = 60.0;
const float INTENSITY_MIN = 0.0;
const float INTENSITY_MAX = 1.0;
const float INTENSITY_DIFF = INTENSITY_MAX - INTENSITY_MIN;

const float INTENSITY_NUMERATOR = INTENSITY_DIFF * DIM_COEF;

float delta_to_intensity(vec2 delta) {
    float length_squared = delta[0] * delta[0] + delta[1] * delta[1];
    float intensity_delta = INTENSITY_NUMERATOR / (length_squared + DIM_COEF);
    return INTENSITY_MIN + intensity_delta;
}

vec4 resolve_visible(vec4 data, int status) {
    vec4 current = vec4(0.0, 0.0, 0.0, 0.0);
    float x_offset = fract(v_CellPos[0]);
    float y_offset = fract(v_CellPos[1]);

    for (int i = 0; i < NUM_TILE_CHANNELS; i++) {
        // check if channel is visible
        if ((status & (1 << (i * STATUS_BITS_PER_CHANNEL + CHANNEL_PRESENT_OFFSET))) == 0) {
            continue;
        }

        bool diminish = (status & (1 << (i * STATUS_BITS_PER_CHANNEL + CHANNEL_DIMINISH_OFFSET))) != 0;

        int word = floatBitsToInt(data[i / 2]);
        if ((i % 2) == 1) {
            word >>= 16;
        }

        int x_coord = word & 0xff;
        int y_coord = (word >> 8) & 0xff;

        float x = (float(x_coord) + x_offset) * u_TexRatio[0];
        float y = (float(y_coord) + y_offset) * u_TexRatio[1];

        vec4 colour = texture(t_Texture, vec2(x, y));

        if (diminish) {
            float intensity = delta_to_intensity(v_CellPos - u_Centre);
            vec3 diminished_colour = vec3(colour) * intensity;
            colour = vec4(diminished_colour, colour[3]);
        }

        current = blend(current, colour);
    }

    return current;
}

const float REMEMBERED_DARKEN = 0.2;

float darken(float x, float coef) {
    return (coef * x * x + 2 * coef * x) / 3.0;
}

vec4 darken_colour(vec4 colour, float coef) {
    float r = darken(colour[0], coef);
    float g = darken(colour[1], coef);
    float b = darken(colour[2], coef);
    return vec4(r, g, b, colour[3]);
}

vec4 resolve_remembered(vec4 data, int status) {
    vec4 coloured = resolve_visible(data, status);
    return darken_colour(coloured, REMEMBERED_DARKEN);
}

void main() {

    int x_idx = int(v_CellPos[0]);
    int y_idx = int(v_CellPos[1]);
    int tile_map_idx = x_idx + y_idx * WIDTH;

    vec4 cell_info = u_Data[tile_map_idx].data;

    int status = floatBitsToInt(cell_info[TILE_STATUS_IDX]);

    if ((status & TILE_STATUS_VISIBLE) != 0) {
        Target0 = resolve_visible(cell_info, status);
    } else {
        Target0 = resolve_remembered(cell_info, status);
    }
}
