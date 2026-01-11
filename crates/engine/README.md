# Threads

## Application Main Thread

This is the thread that the main application UI runs on - usually the same thread that main() ran on.

## Plugin Host Thread

This is the thread that plugins consider the "main" thread.  The plugin UI runs on this thread.

## Audio Thread

This is the thread that CPAL uses for the audio callbacks.
