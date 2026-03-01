# KF2 Architecture

KF2 is comprised of four main components:
- The server process, running locally or remotely, which stores session state, interacts with upstream data providers, and exposes various session management APIs. The server process also serves the various user-facing interfaces.
- The projector single-page app, which renders the karaoke video, lyrics, and any other ancillary information as necessary (e.g. the queue, the remocon QR code).
- The remocon single-page app, which allows users to control playback and manage the song queue. 
- The optional native audio helper, designed to be run alongside the projector single-page app. The process handles microphone monitoring, pitch analysis, and advanced audio processing (e.g. VSTs, pitch shifting).

```mermaid
flowchart LR

subgraph Server
    subgraph Session
        session-state[Session State]
        queue[Queue]
    end

    session-state --> persistent-storage
    persistent-storage@{ shape: lin-cyl, label: "Persistent storage" }

    subgraph Asset Server
        asset-api[Asset API]
        asset-storage@{ shape: lin-cyl, label: "Asset storage" }

        asset-api <-- cache --> asset-storage
    end
end

subgraph Projector
    render[Video Rendering]
    queue-display[Queue Display]
end

subgraph Remocon
    playback-controls[Playback Controls]
    song-search[Song Search]
end

subgraph "Native Audio (Optional)"
    direction LR
    mic@{ shape: notch-rect, label: "Microphones" }
    speaker@{ shape: notch-rect, label: "Speakers" }
    pitch-detection@{ shape: subproc, label: "Pitch Detection" }
    pitch-shift@{ shape: subproc, label: "Pitch Shifting" }

    mic --> speaker
    mic --> pitch-detection
    pitch-detection -- piano roll --> render
    pitch-shift --> speaker
end

subgraph External Data Providers
    direction LR
    karaoke-provider@{ shape: lin-rect, label: "Karaoke Data Providers" }
end

render -- controlled by --> session-state
render -- sinks audio --> pitch-shift
render -- fetches from --> asset-api
queue-display -- shows --> queue
queue -- stored in --> session-state
playback-controls -- controls --> session-state
song-search -- queries --> asset-api
song-search -- modifies --> queue

asset-api <-- backed by --> karaoke-provider

```
