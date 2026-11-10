#version 400

in vec3 vWorldPos;
in vec3 vNormal;
in vec2 vUv;
in vec3 vLocalPos;

uniform vec3 uColor;
uniform vec3 uCameraPos;
uniform float uTime;
uniform int uMaterial;      // 0 board square, 1 chess piece, 2 floor grid, 3 overlay, 4 move highlight, 5 board base
uniform float uAlpha;
uniform int uReflection;

out vec4 fragColor;

float saturate(float x)
{
    return clamp(x, 0.0, 1.0);
}

float gridLine(vec2 p, float scale, float thickness)
{
    vec2 q = p * scale;
    vec2 grid = abs(fract(q - 0.5) - 0.5) / fwidth(q);
    float line = min(grid.x, grid.y);
    return 1.0 - saturate(line - thickness);
}

vec3 proceduralEnvironment(vec3 r)
{
    float top = saturate(r.y * 0.5 + 0.5);
    vec3 dark = vec3(0.010, 0.018, 0.030);
    vec3 blue = vec3(0.050, 0.380, 0.950);
    vec3 gold = vec3(1.000, 0.640, 0.180);
    float band = 0.5 + 0.5 * sin(8.0 * r.x + 5.0 * r.z + uTime * 0.45);
    return mix(dark, mix(blue, gold, band * 0.34), top);
}

vec3 shadeSurface(vec3 baseColor, float roughness, float reflectionBoost)
{
    vec3 n = normalize(vNormal);
    vec3 v = normalize(uCameraPos - vWorldPos);
    if (dot(n, v) < 0.0) n = -n;

    vec3 l1 = normalize(vec3(-0.45, 0.90, 0.34));
    vec3 l2 = normalize(vec3(0.70, 0.30, -0.60));

    float diff1 = max(dot(n, l1), 0.0);
    float diff2 = max(dot(n, l2), 0.0) * 0.42;

    vec3 h1 = normalize(l1 + v);
    vec3 h2 = normalize(l2 + v);

    float specPower = mix(120.0, 24.0, roughness);
    float spec1 = pow(max(dot(n, h1), 0.0), specPower);
    float spec2 = pow(max(dot(n, h2), 0.0), specPower * 0.75) * 0.55;

    float ndv = max(dot(n, v), 0.0);
    float fresnel = pow(1.0 - ndv, 4.0);
    float rim = pow(1.0 - ndv, 2.2);

    vec3 r = reflect(-v, n);
    vec3 env = proceduralEnvironment(r);

    vec3 ambient = baseColor * vec3(0.18, 0.20, 0.25);
    vec3 diffuse = baseColor * (diff1 + diff2) * 0.98;
    vec3 specular = (spec1 + spec2) * mix(vec3(1.0), env, 0.22 + reflectionBoost);
    vec3 reflected = env * fresnel * (0.16 + reflectionBoost);
    vec3 rimColor = vec3(0.05, 0.55, 1.0) * rim * 0.62;

    return ambient + diffuse + specular + reflected + rimColor;
}

void main()
{
    if (uMaterial == 3)
    {
        fragColor = vec4(uColor, uAlpha);
        return;
    }

    if (uMaterial == 2)
    {
        vec2 p = (vUv - vec2(0.5)) * 18.0;
        float minor = gridLine(p, 1.0, 0.65);
        float major = gridLine(p, 0.2, 0.85);
        float axisX = 1.0 - smoothstep(0.0, 0.025, abs(p.x));
        float axisZ = 1.0 - smoothstep(0.0, 0.025, abs(p.y));
        float fade = exp(-length(p) * 0.13);

        vec3 base = vec3(0.004, 0.007, 0.012);
        vec3 lineColor = vec3(0.02, 0.46, 0.95) * minor * 0.58;
        lineColor += vec3(0.18, 0.78, 1.00) * major * 1.05;
        lineColor += vec3(0.30, 0.95, 1.00) * max(axisX, axisZ) * 0.85;

        vec3 color = base + lineColor * fade;
        fragColor = vec4(color, uAlpha * fade);
        return;
    }

    if (uMaterial == 4)
    {
        float pulse = 0.65 + 0.35 * sin(uTime * 5.0);
        vec3 color = uColor * (0.45 + pulse * 0.65);
        color += vec3(0.05, 0.35, 0.85) * pulse;
        fragColor = vec4(color, uAlpha);
        return;
    }

    if (uMaterial == 5)
    {
        vec3 color = shadeSurface(uColor, 0.24, 0.52);
        color *= 0.78;
        fragColor = vec4(color, uAlpha);
        return;
    }

    if (uMaterial == 1)
    {
        vec3 base = uColor;
        float rings = 0.5 + 0.5 * sin(vWorldPos.y * 24.0 + vWorldPos.x * 7.0 + vWorldPos.z * 5.0);
        base *= 0.94 + rings * 0.035;
        vec3 color = shadeSurface(base, 0.18, 0.44);
        float scan = smoothstep(0.018, 0.000, abs(fract(vWorldPos.y * 0.58 - uTime * 0.15) - 0.5));
        color += vec3(0.04, 0.42, 1.0) * scan * 0.10;
        if (uReflection == 1) color *= vec3(0.42, 0.70, 1.0);
        fragColor = vec4(color, uAlpha);
        return;
    }

    vec3 squareColor = uColor;
    float micro = 0.5 + 0.5 * sin((vWorldPos.x * 11.0 + vWorldPos.z * 13.0) + sin(vWorldPos.z * 8.0));
    squareColor *= 0.95 + micro * 0.05;

    vec3 color = shadeSurface(squareColor, 0.22, 0.20);
    float edge = max(abs(vLocalPos.x), abs(vLocalPos.z));
    float edgeMask = smoothstep(0.38, 0.50, edge);
    color += vec3(0.03, 0.30, 0.70) * edgeMask * 0.10;
    fragColor = vec4(color, uAlpha);
}
