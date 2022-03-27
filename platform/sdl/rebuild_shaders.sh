#!/usr/bin/sh
for entry in ../../engine/shaders/*
do
  filename=$(basename $entry)
  if [[ "$filename" == *.glsl ]]
  then
    echo "Compiling: $filename"
    glslangValidator -V "$entry" --target-env spirv1.4 -o ./$(basename -s glsl $filename)spv
  fi
done
