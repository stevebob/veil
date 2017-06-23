#version 150 core

in vec2 v_TexPos;

out vec4 Target0;

uniform sampler2D t_Texture;

void main() {
    Target0 = texture(t_Texture, v_TexPos);
}