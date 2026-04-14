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

# Design Philosophy by Component

## Backend Server

- The backend server should never (or rarely) crash. If it does, no state should be lost or corrupted to a state that requires manipulation outside the boundaries of the UI (via API).
- The backend server should be solely responsible for the state of a session, beyond the scope of playing a single song.
- As much as possible, complex manipulation and logic should happen within the backend. However, the UI should still be responsive and provide the user with sufficient feedback as to the status of their request. Whenever possible, long-running operations should be asynchronous and idempotent.

## Projector

- The projector should itself be relatively stateless.
- All state should be fetched from the backend, and the backend should be responsible for ensuring that each session only has one active projector instance.

## Remocon

- Similar constraints on maintaining state to the projector, however, per-user settings may be stored in local storage.

## Native Audio Helper

- Low audio latency is key to the karaoke experience. As such, wherever possible, parameters should be configured such that the lowest possible audio latency is achieved, while also keeping stability in mind.
- It may be necessary to manually adjust parameters to achieve low latency. To this end, the native audio helper should provide sufficient instrumentation to allow an experienced user to make the appropriate decisions.
- Native, platform-specific code interfacing with low-level audio APIs can be complex and hard-to-verify. To counteract this, the helper should be designed in a modular fashion that allows components to be mocked out for easier testing.