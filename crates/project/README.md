# Project

This crate provides the main data model for a corodaw project.

## Channels

There are three types of channels:
* audio
* midi
* group

The type determines what the audio source is for the channel:

| Type | Source |
|------|--------|
| Audio | Audio clips on audio tracks |
| MIDI | Output from generator plugin |
| Group | Mixed output from all sub-channels |

Things that can be in a channel:

| Thing             | Audio | MIDI | Group |
|-------------------|-------|------|-------|
| Plugin Chain      | ✅ | ✅ | ✅ |
| Automation Tracks | ✅ | ✅ | ✅ |
| Audio Tracks      | ✅ | ❌ | ❌ |
| Generator Plugin  | ❌ | ✅ | ❌ |
| MIDI Tracks       | ✅ | ✅ | ❌ |
| Channels          | ❌ | ❌ | ✅ |

Although it could be well defined what would happen if, say, an audio track was
added to a MIDI channel, the UI for this may well be confusing.

## Plugin Chain

The plugin chain is the list of plugins that the audio signal flows through. The
audio starts with the audio clips, or from the generator plugin, and is passed
one-by-one through the chain.

The plugin chain includes the fader and any standard mixer controls (eq etc.)

## Automation Tracks

Each automation track is tied to a single parameter from a plugin in the
channel, and contains automation clips.

## Audio Tracks

Audio tracks contain audio clips and provide the source of the audio signal for
an audio channel. Multiple audio tracks can be added, the audio from these are
mixed before being passed to the plugin chain.

## Generator Plugin

This plugin generates audio, and is the default destination for any MIDI tracks.

## MIDI Tracks

MIDI tracks contain MIDI clips. Each MIDI track has a designated destination
plugin - this defaults to the generator plugin. Multiple MIDI tracks can target
the same plugin and the events coming from these tracks are merged before being
passed to the destination plugin.

## Channels

Sub-channels are full channels in their own right.
