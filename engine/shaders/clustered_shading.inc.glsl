#ifndef CLUSTERED_SHADING_H
#define CLUSTERED_SHADING_H

uint getClusterIndex(vec2 fragCoord, float z, uvec3 clusterCount, uvec2 viewportSize, float clusterZScale, float clusterZBias) {
  vec2 tileSize = vec2(viewportSize) / vec2(clusterCount.xy);

  uvec3 clusterIndex3d = uvec3(
    uint(fragCoord.x / tileSize.x),
    uint(fragCoord.y / tileSize.y),
    uint(max(0.0, log2(z) * clusterZScale + clusterZBias))
  );

  uint clusterIndex = clusterIndex3d.x +
                    clusterIndex3d.y * clusterCount.x +
                    clusterIndex3d.z * (clusterCount.x * clusterCount.y);

  #ifdef DEBUG
  if (abs(z - viewPos.z) > 0.01) {
    debugPrintfEXT("Wrong z: %f, expected: %f", z, viewPos.z);
  }
  #endif

  return clusterIndex;
}

uint getClusterIndexWithDepth(vec2 fragCoord, float depth, float zNear, float zFar, uvec3 clusterCount, uvec2 viewportSize, float clusterZScale, float clusterZBias) {
  float z = linearizeDepth(depth, zNear, zFar);
  return getClusterIndex(fragCoord, z, clusterCount, viewportSize, clusterZScale, clusterZBias);
}

#ifdef DEBUG
bool validateCluster(vec3 viewPos, Cluster cluster) {
  vec3 viewPos = (camera.view * vec4(worldPos, 1)).xyz;

  return !(viewPos.x > cluster.maxPoint.x + 0.01 || viewPos.x < c.minPoint.x - 0.01
  || viewPos.y > cluster.maxPoint.y + 0.01 || viewPos.y < c.minPoint.y - 0.01
  || viewPos.z > cluster.maxPoint.z + 0.01 || viewPos.z < c.minPoint.z - 0.01);
}
#endif

#endif
