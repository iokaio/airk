#version 450
// Original receipt vertex shader 🌱.
layout(location = 0) in vec3 position;
layout(binding = 1) uniform sampler2D receipt;
layout(std140, binding = 2) uniform Ledger { vec4 amount; } ledger;
void main() { gl_Position = vec4(position, 1.0); }
