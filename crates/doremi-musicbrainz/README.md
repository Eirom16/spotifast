# Doremi MusicBrainz metadata adapter

Public search for recordings, artists and releases, artist recordings, and
release track lists. No audio resolver, credentials, cookies or process runtime.
MusicBrainz IDs retain their entity type and `musicbrainz` namespace.

Requests use the official API, an identifying User-Agent, one process-wide
request slot every 1.1 seconds, no redirects, 15-second total deadlines,
1 MiB responses, pages of 20 and offsets up to 1000. A release over 500 tracks
is rejected explicitly. Eight cached pages (up to 160 entries), five-minute
TTL, in memory only. Cancellation drops the request future; the host owns
bounded channels and generation checks. No external requests in unit tests.

Only core metadata fields are retained. No tags, ratings, aliases, secrets or
provider responses enter persisted queues. Metadata does not confer audio
rights. MusicBrainz does not supply cover art through this adapter.

Primary references:
[API](https://musicbrainz.org/doc/MusicBrainz_API),
[data licenses](https://musicbrainz.org/doc/About/Data_License).
The public API describes free non-commercial use and commercial plans;
check those terms before a commercial distribution. Core data is CC0;
supplementary data has different terms and is not imported here.
