#!/usr/bin/env python3
from http.server import HTTPServer, SimpleHTTPRequestHandler
import sys

class RequestHandler(SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        self.extensions_map = {
          '.manifest': 'text/cache-manifest',
          '.html': 'text/html',
          '.png': 'image/png',
          '.jpg': 'image/jpg',
          '.svg': 'image/svg+xml',
          '.css': 'text/css',
          '.js': 'application/x-javascript',
          '.gltf': 'text/json',
          '': 'application/octet-stream', # Default
        }
        super().__init__(*args, directory='dist', **kwargs)

    def translate_path(self, path: str) -> str:
        if path.__contains__("/assets/"):
           return "../../.." + path
        else:
            return super().translate_path(path)

    def end_headers(self):
        self.send_header('Cross-Origin-Opener-Policy', 'same-origin')
        self.send_header('Cross-Origin-Embedder-Policy', 'require-corp')
        SimpleHTTPRequestHandler.end_headers(self)

if __name__ == '__main__':
    with HTTPServer(('localhost', 8080), RequestHandler) as httpd:
        httpd.serve_forever()
