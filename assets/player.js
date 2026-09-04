window.MusicApp = (function() {
    function fmtTime(seconds) {
        if (!seconds || isNaN(seconds)) return "0:00";
        const m = Math.floor(seconds / 60);
        const sec = Math.floor(seconds % 60);
        return m + ":" + (sec < 10 ? "0" + sec : sec);
    }

    function getAudio() {
        return document.getElementById('audio-player');
    }

    function getVideo() {
        return document.getElementById('track-visual-video');
    }

    function getProgressBar() {
        return document.getElementById('music-progress-bar');
    }

    function getCurrentTimeLabel() {
        return document.getElementById('music-current-time');
    }

    function getDurationLabel() {
        return document.getElementById('music-duration-time');
    }

    function setVolume(volume) {
        const audio = getAudio();
        const slider = document.querySelector('.volume-slider');
        if (audio) {
            audio.volume = volume;
        }
        if (slider) {
            slider.value = volume;
        }
    }

    function seekAudio(offsetMs) {
        const audio = getAudio();
        const progress = getProgressBar();
        if (!audio || !progress || isNaN(audio.duration) || audio.duration <= 0) return;

        const pct = Math.max(0, Math.min(100, parseFloat(progress.value)));
        const displayTime = (audio.duration * pct) / 100;
        const audioDelaySec = Math.max(0, -offsetMs / 1000);
        const newTime = Math.min(audio.duration, displayTime + audioDelaySec);
        audio.currentTime = newTime;

        if (progress) {
            progress.style.background = `linear-gradient(to right, #22c55e ${pct}%, #38383e ${pct}%)`;
        }
    }

    function startVideoDelayed(delayMs) {
        const video = getVideo();
        if (!video) return;

        setTimeout(() => {
            video.currentTime = 0;
            video.play().catch(e => console.log("Play error:", e));
        }, delayMs);
    }

    function setupTrack({ trackId, audioSrc, videoSrc, offsetMs, playing }) {
        const audio = getAudio();
        const video = getVideo();

        if (!audio) return;

        window.__music_player_token = (window.__music_player_token || 0) + 1;
        const token = window.__music_player_token;

        if (window.__music_video_start_timer) {
            clearTimeout(window.__music_video_start_timer);
            window.__music_video_start_timer = null;
        }
        if (window.__music_video_wait_timer) {
            clearInterval(window.__music_video_wait_timer);
            window.__music_video_wait_timer = null;
        }
        if (window.__music_audio_start_timer) {
            clearTimeout(window.__music_audio_start_timer);
            window.__music_audio_start_timer = null;
        }

        const videoDelayMs = Math.max(offsetMs, 0);
        const audioDelayMs = Math.max(-offsetMs, 0);

        const previousTrackId = Number(audio.dataset.trackId ?? "-1");
        const trackChanged = previousTrackId !== trackId;

        audio.dataset.trackId = String(trackId);

        function isCurrentRun() {
            return window.__music_player_token === token;
        }

        function stopVideo() {
            if (!video) return;
            video.pause();
            try {
                video.currentTime = 0;
            } catch (_) {}
        }

        function startAudio() {
            if (!isCurrentRun() || !playing) return;
            audio.play().catch(() => {});
        }

        function startVideo() {
            if (!video || videoSrc === "") return;
            if (!isCurrentRun() || !playing) return;

            if (video.readyState < 2) {
                window.__music_video_wait_timer = setInterval(() => {
                    if (!isCurrentRun() || !playing) {
                        clearInterval(window.__music_video_wait_timer);
                        window.__music_video_wait_timer = null;
                        return;
                    }
                    if (video.readyState >= 2) {
                        clearInterval(window.__music_video_wait_timer);
                        window.__music_video_wait_timer = null;
                        try {
                            video.currentTime = 0;
                        } catch (_) {}
                        video.play().catch(() => {});
                    }
                }, 25);
                return;
            }

            try {
                video.currentTime = 0;
            } catch (_) {}
            video.play().catch(() => {});
        }

        if (trackChanged) {
            audio.pause();
            try {
                audio.currentTime = 0;
            } catch (_) {}

            audio.dataset.src = audioSrc;
            audio.dataset.ready = "0";

            const progress = getProgressBar();
            const currentLabel = getCurrentTimeLabel();
            const durationLabel = getDurationLabel();

            if (progress) {
                progress.value = 0;
                progress.style.background = 'linear-gradient(to right, #22c55e 0%, #38383e 0%)';
            }
            if (currentLabel) currentLabel.innerText = '0:00';
            if (durationLabel) durationLabel.innerText = '0:00';

            audio.onloadedmetadata = () => {
                if (!isCurrentRun()) return;
                audio.dataset.ready = "1";
                const duration = audio.duration;
                if (durationLabel && duration && !isNaN(duration)) {
                    durationLabel.innerText = fmtTime(duration);
                }
                if (playing) {
                    if (audioDelayMs === 0) {
                        startAudio();
                    } else {
                        window.__music_audio_start_timer = setTimeout(() => {
                            window.__music_audio_start_timer = null;
                            startAudio();
                        }, audioDelayMs);
                    }
                }
            };

            audio.src = audioSrc;
            audio.load();
        } else {
            if (playing) {
                if (video && videoSrc !== "") {
                    video.play().catch(() => {});
                }
                audio.play().catch(() => {});
            } else {
                audio.pause();
                if (video) video.pause();
            }
        }

        if (trackChanged && video && videoSrc !== "") {
            const videoChanged = video.dataset.src !== videoSrc;
            if (videoChanged) {
                video.dataset.src = videoSrc;
                video.src = videoSrc;
                video.load();
            }
            stopVideo();
            if (playing) {
                if (videoDelayMs === 0) {
                    startVideo();
                } else {
                    window.__music_video_start_timer = setTimeout(() => {
                        window.__music_video_start_timer = null;
                        startVideo();
                    }, videoDelayMs);
                }
            }
        } else if (trackChanged && (!video || videoSrc === "")) {

        }

        if (!playing) {
            audio.pause();
            if (video) video.pause();
        }
    }

    function updateProgress(offsetMs) {
        const audio = getAudio();
        const video = getVideo();
        const progress = getProgressBar();
        const currentLabel = getCurrentTimeLabel();
        const durationLabel = getDurationLabel();

        if (!audio || !audio.duration || isNaN(audio.duration)) return;

        const offsetSec = offsetMs / 1000;
        const displayTime = Math.max(0, Math.min(audio.duration, audio.currentTime + Math.min(offsetSec, 0)));
        const pct = (displayTime / audio.duration) * 100;

        if (progress) {
            progress.value = pct;
            progress.style.background = `linear-gradient(to right, #22c55e ${pct}%, #38383e ${pct}%)`;
        }
        if (currentLabel) currentLabel.innerText = fmtTime(displayTime);
        if (durationLabel) durationLabel.innerText = fmtTime(audio.duration);

        if (video) {
            const expectedVideoTime = audio.currentTime - offsetSec;
            if (expectedVideoTime <= 0) {
                if (!video.paused) video.pause();
                if (video.currentTime > 0.02) {
                    try {
                        video.currentTime = 0;
                    } catch (_) {}
                }
            } else {
                const drift = Math.abs(video.currentTime - expectedVideoTime);
                if (drift > 0.20) {
                    try {
                        video.currentTime = expectedVideoTime;
                    } catch (_) {}
                }
            }
        }
    }

    return {
        fmtTime,
        setVolume,
        seekAudio,
        setupTrack,
        updateProgress,
        startVideoDelayed
    };
})();