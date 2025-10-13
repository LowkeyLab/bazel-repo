#!/usr/bin/env bash

# Build the Angular app
echo "Building Angular application..."
bazel build //angular-test:app

if [ $? -eq 0 ]; then
    echo "Build successful! Starting development server..."
    echo "Open http://localhost:8080 in your browser"
    echo "Press Ctrl+C to stop the server"
    cd ../bazel-bin/angular-test/prod && python3 -m http.server 8080
else
    echo "Build failed!"
    exit 1
fi
