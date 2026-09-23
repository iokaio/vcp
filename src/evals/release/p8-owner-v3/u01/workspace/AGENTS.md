# Project guidance

Maintained source is under src/. app coordinates domain and storage. transport calls app. Domain modules may import only domain or shared modules. Storage must not depend on transport. generated/ is machine output and is not evidence of maintained source dependency direction. Use narrower instructions when entering a directory. Analysis is read-only.
