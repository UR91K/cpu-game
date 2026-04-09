```c
// ntsc-param.inc
#define PI 3.14159265

#if defined(TWO_PHASE)
#define CHROMA_MOD_FREQ (4.0 * PI / 15.0)
#elif defined(THREE_PHASE)
#define CHROMA_MOD_FREQ (PI / 3.0)
#endif

#if defined(COMPOSITE)
#define SATURATION 1.0
#define BRIGHTNESS 1.0
#define ARTIFACTING 1.0
#define FRINGING 1.0
#elif defined(SVIDEO)
#define SATURATION 1.0
#define BRIGHTNESS 1.0
#define ARTIFACTING 0.0
#define FRINGING 0.0
#endif

#if defined(COMPOSITE) || defined(SVIDEO)
mat3 mix_mat = mat3(
	BRIGHTNESS, FRINGING, FRINGING,
	ARTIFACTING, 2.0 * SATURATION, 0.0,
	ARTIFACTING, 0.0, 2.0 * SATURATION
);
#endif

```

```c
// ntsc-rgbyuv.inc
const mat3 yiq2rgb_mat = mat3(
   1.0, 0.956, 0.6210,
   1.0, -0.2720, -0.6474,
   1.0, -1.1060, 1.7046);

vec3 yiq2rgb(vec3 yiq)
{
   return yiq * yiq2rgb_mat;
}

const mat3 yiq_mat = mat3(
      0.2989, 0.5870, 0.1140,
      0.5959, -0.2744, -0.3216,
      0.2115, -0.5229, 0.3114
);

vec3 rgb2yiq(vec3 col)
{
   return col * yiq_mat;
}
```

```c
// ntsc-pass1-vertex.inc
#pragma stage vertex
layout(location = 0) in vec4 Position;
layout(location = 1) in vec2 TexCoord;
layout(location = 0) out vec2 vTexCoord;
layout(location = 1) out vec2 pix_no;

void main()
{
   gl_Position = global.MVP * Position;
   vTexCoord = TexCoord;
   pix_no = TexCoord * global.SourceSize.xy * (global.OutputSize.xy / global.SourceSize.xy);
}
```

```c
// ntsc-pass1-encode-demodulate.inc
vec3 col = texture(Source, vTexCoord).rgb;
vec3 yiq = rgb2yiq(col);

#if defined(TWO_PHASE)
float chroma_phase = PI * (mod(pix_no.y, 2.0) + global.FrameCount);
#elif defined(THREE_PHASE)
float chroma_phase = 0.6667 * PI * (mod(pix_no.y, 3.0) + global.FrameCount);
#endif

float mod_phase = chroma_phase + pix_no.x * CHROMA_MOD_FREQ;

float i_mod = cos(mod_phase);
float q_mod = sin(mod_phase);

yiq.yz *= vec2(i_mod, q_mod); // Modulate.
yiq *= mix_mat; // Cross-talk.
yiq.yz *= vec2(i_mod, q_mod); // Demodulate.
FragColor = vec4(yiq, 1.0);

```

```c
// ntsc-pass1-composite-2phase.slang

#version 450

layout(std140, set = 0, binding = 0) uniform UBO
{
   mat4 MVP;
   vec4 OutputSize;
   vec4 OriginalSize;
   vec4 SourceSize;
   uint FrameCount;
} global;

#define TWO_PHASE
#define COMPOSITE
#include "ntsc-param.inc"
#include "ntsc-rgbyuv.inc"

#include "ntsc-pass1-vertex.inc"

#pragma stage fragment
layout(location = 0) in vec2 vTexCoord;
layout(location = 1) in vec2 pix_no;
layout(location = 0) out vec4 FragColor;
layout(set = 0, binding = 2) uniform sampler2D Source;

void main()
{
#include "ntsc-pass1-encode-demodulate.inc"
}
```

```c
// ntsc-decode-filter-2phase.inc

#define TAPS 32
const float luma_filter[TAPS + 1] = float[TAPS + 1](
   -0.000174844,
   -0.000205844,
   -0.000149453,
   -0.000051693,
   0.000000000,
   -0.000066171,
   -0.000245058,
   -0.000432928,
   -0.000472644,
   -0.000252236,
   0.000198929,
   0.000687058,
   0.000944112,
   0.000803467,
   0.000363199,
   0.000013422,
   0.000253402,
   0.001339461,
   0.002932972,
   0.003983485,
   0.003026683,
   -0.001102056,
   -0.008373026,
   -0.016897700,
   -0.022914480,
   -0.021642347,
   -0.008863273,
   0.017271957,
   0.054921920,
   0.098342579,
   0.139044281,
   0.168055832,
   0.178571429);

const float chroma_filter[TAPS + 1] = float[TAPS + 1](
   0.001384762,
   0.001678312,
   0.002021715,
   0.002420562,
   0.002880460,
   0.003406879,
   0.004004985,
   0.004679445,
   0.005434218,
   0.006272332,
   0.007195654,
   0.008204665,
   0.009298238,
   0.010473450,
   0.011725413,
   0.013047155,
   0.014429548,
   0.015861306,
   0.017329037,
   0.018817382,
   0.020309220,
   0.021785952,
   0.023227857,
   0.024614500,
   0.025925203,
   0.027139546,
   0.028237893,
   0.029201910,
   0.030015081,
   0.030663170,
   0.031134640,
   0.031420995,
   0.031517031);
```

```c
// ntsc-pass2-vertex.inc
#pragma stage vertex
layout(location = 0) in vec4 Position;
layout(location = 1) in vec2 TexCoord;
layout(location = 0) out vec2 vTexCoord;

void main()
{
   gl_Position = global.MVP * Position;
   vTexCoord = TexCoord - vec2(0.5 / global.SourceSize.x, 0.0); // Compensate for decimate-by-2.
}
```

```c
// ntsc-pass2-decode.inc

float one_x = 1.0 / global.SourceSize.x;
vec3 signal = vec3(0.0);
for (int i = 0; i < TAPS; i++)
{
   float offset = float(i);

   vec3 sums = fetch_offset(offset - float(TAPS), one_x) +
      fetch_offset(float(TAPS) - offset, one_x);

   signal += sums * vec3(luma_filter[i], chroma_filter[i], chroma_filter[i]);
}
signal += texture(Source, vTexCoord).xyz *
   vec3(luma_filter[TAPS], chroma_filter[TAPS], chroma_filter[TAPS]);
```

```c
// ntsc-pass2-2phase-gamma.slang

#version 450

layout(std140, set = 0, binding = 0) uniform UBO
{
   mat4 MVP;
   vec4 OutputSize;
   vec4 OriginalSize;
   vec4 SourceSize;
} global;

#include "ntsc-rgbyuv.inc"
#include "ntsc-decode-filter-2phase.inc"

#define fetch_offset(offset, one_x) \
   texture(Source, vTexCoord + vec2((offset) * (one_x), 0.0)).xyz

#define NTSC_CRT_GAMMA 2.5
#define NTSC_MONITOR_GAMMA 2.0

#include "ntsc-pass2-vertex.inc"

#pragma stage fragment
layout(location = 0) in vec2 vTexCoord;
layout(location = 0) out vec4 FragColor;
layout(set = 0, binding = 2) uniform sampler2D Source;

void main()
{
#include "ntsc-pass2-decode.inc"
vec3 rgb = yiq2rgb(signal);
FragColor = vec4(pow(rgb, vec3(NTSC_CRT_GAMMA / NTSC_MONITOR_GAMMA)), 1.0);
}

```

```c
// ntsc-composite.slang
# Resolution-independent NTSC composite shader
# Works with arbitrary input resolutions
# Pass 0: Encodes to NTSC composite signal at 4x horizontal resolution
# Pass 1: Decodes back to RGB at 2x horizontal resolution (for CRT-like output)

shaders = 2
shader0 = ntsc-pass1-composite-2phase.slang
shader1 = ntsc-pass2-2phase-gamma.slang

filter_linear0 = false
filter_linear1 = false

# Pass 0: Scale horizontally by 4x for NTSC composite signal generation
scale_type_x0 = source
scale_type_y0 = source
scale_x0 = 4.0
scale_y0 = 1.0
frame_count_mod0 = 2
float_framebuffer0 = true

# Pass 1: Scale back to 2x horizontal (net result: 2x the input width)
scale_type1 = source
scale_x1 = 0.5
scale_y1 = 1.0

```