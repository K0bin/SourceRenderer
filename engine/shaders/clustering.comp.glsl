// http://www.aortiz.me/2018/12/21/CG.html#building-a-cluster-grid
// https://github.com/pezcode/Cluster/blob/master/src/Renderer/Shaders/cs_clustered_clusterbuilding.sc

#version 450
#extension GL_GOOGLE_include_directive : enable
// #extension GL_EXT_debug_printf : enable

layout(local_size_x = 8, local_size_y = 1, local_size_z = 8) in;

#include "descriptor_sets.inc.glsl"
#include "util.inc.glsl"
#include "camera.inc.glsl"

struct VolumeTileAABB{
  vec4 minPoint;
  vec4 maxPoint;
};
layout(std430, set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 0, std430) writeonly buffer clusterAABB {
  VolumeTileAABB cluster[];
};
layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 1, std430) buffer setupBuffer {
  uvec2 tileSize;
  uvec2 screenDimensions;
  float zNear;
  float zFar;
};

layout(set = DESCRIPTOR_SET_VERY_FREQUENT, binding = 2, std140) uniform CameraUBO {
  Camera camera;
};

// Function prototypes
vec3 lineIntersectionToZPlane(vec3 A, vec3 B, float zDistance);

void main() {
  // Eye position is zero in view space
  const vec3 eyePos = vec3(0.0);

  // Per Tile variables
  uint clusterIndex = gl_GlobalInvocationID.x +
                      gl_GlobalInvocationID.y * gl_WorkGroupSize.x * gl_NumWorkGroups.x+
                      gl_GlobalInvocationID.z * gl_WorkGroupSize.x * gl_NumWorkGroups.x * gl_WorkGroupSize.y * gl_NumWorkGroups.y;

  // Calculating the min and max point in screen space
  vec2 maxPoint_sS = vec2((gl_GlobalInvocationID.xy + uvec2(1)) * tileSize); // Top Right
  vec2 minPoint_sS = vec2(gl_GlobalInvocationID.xy * tileSize); // Bottom left

  // Pass min and max to view space
  vec3 maxPoint_vS = viewSpacePosition(maxPoint_sS / screenDimensions, 0.0, camera.invProj).xyz;
  vec3 minPoint_vS = viewSpacePosition(minPoint_sS / screenDimensions, 0.0, camera.invProj).xyz;

  // Near and far values of the cluster in view space
  float tileNear  = zNear * pow(zFar / zNear, gl_GlobalInvocationID.z / float(gl_NumWorkGroups.z * gl_WorkGroupSize.z));
  float tileFar   = zNear * pow(zFar / zNear, (gl_GlobalInvocationID.z + 1) / float(gl_NumWorkGroups.z * gl_WorkGroupSize.z));

  // Finding the 4 intersection points made from the maxPoint to the cluster near/far plane
  vec3 minPointNear = lineIntersectionToZPlane(eyePos, minPoint_vS, tileNear);
  vec3 minPointFar  = lineIntersectionToZPlane(eyePos, minPoint_vS, tileFar);
  vec3 maxPointNear = lineIntersectionToZPlane(eyePos, maxPoint_vS, tileNear);
  vec3 maxPointFar  = lineIntersectionToZPlane(eyePos, maxPoint_vS, tileFar);

  vec3 minPointAABB = min(min(minPointNear, minPointFar), min(maxPointNear, maxPointFar));
  vec3 maxPointAABB = max(max(minPointNear, minPointFar), max(maxPointNear, maxPointFar));

  cluster[clusterIndex].minPoint = vec4(minPointAABB, 0.0);
  cluster[clusterIndex].maxPoint = vec4(maxPointAABB, 0.0);
}

// Creates a line from the eye to the screenpoint, then finds its intersection
// With a z oriented plane located at the given distance to the origin
vec3 lineIntersectionToZPlane(vec3 A, vec3 B, float zDistance) {
  // Because this is a Z based normal this is fixed
  vec3 normal = vec3(0.0, 0.0, 1.0);

  vec3 ab =  B - A;

  // Computing the intersection length for the line and the plane
  float t = (zDistance - dot(normal, A)) / dot(normal, ab);

  // Computing the actual xyz position of the point along the line
  vec3 result = A + t * ab;

  return result;
}
