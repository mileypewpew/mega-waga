// Waga HumanMusic live bed — SuperCollider
// Run: sclang assets/sc/waga_bed.scd
// Listens for OSC on port 57120 (sclang default):
//   /waga/tension  f   0..1
//   /waga/bpm      f
//   /waga/mood     s
//   /waga/story_open i
//   /waga/git_dirty  i
//   /waga/title    s
//   /waga/gate     i   0=soft mute, 1=open

(
s.waitForBoot({
    ~waga = (
        tension: 0.2,
        bpm: 90,
        mood: "idle",
        story_open: 0,
        git_dirty: 0,
        gate: 1,
        title: "Waga HumanMusic Bed"
    );

    SynthDef(\wagaPad, { |out=0, freq=110, amp=0.08, cutoff=800, rq=0.4, gate=1|
        var env = EnvGen.kr.ar(Env.asr(2.0, 1, 3.0), gate, doneAction: 0);
        var det = [0, 0.3, -0.4].midiratio;
        var osc = Mix.fill(3, { |i|
            Saw.ar(freq * det[i] * LFNoise1.kr.ar(0.1).range(0.995, 1.005))
        }) * 0.3;
        var f = RLPF.ar(osc, cutoff.clip(200, 8000), rq);
        var pan = Pan2.ar(f * amp * env, LFNoise1.kr.ar(0.05));
        Out.ar(out, pan);
    }).add;

    SynthDef(\wagaPulse, { |out=0, freq=220, amp=0.03, gate=1|
        var env = EnvGen.kr.ar(Env.perc(0.01, 0.25), gate, doneAction: 0);
        var sig = Pulse.ar(freq, 0.3) * env * amp;
        Out.ar(out, Pan2.ar(sig, 0));
    }).add;

    s.sync;

    ~pad = Synth(\wagaPad, [\freq, 110, \amp, 0.06, \cutoff, 600]);
    ~clock = TempoClock.new(90/60);

    ~apply = {
        var t = ~waga[\tension].clip(0, 1);
        var base = 90 + (t * 50); // map toward bpm-ish center
        var freq = 90 * (1 + (t * 0.8));
        var cut = 400 + (t * 3500);
        var amp = 0.03 + (t * 0.1);
        if(~waga[\gate] == 0) { amp = amp * 0.05 };
        ~pad.set(\freq, freq, \cutoff, cut, \amp, amp);
        ~clock.tempo = (~waga[\bpm] / 60).clip(0.8, 2.8);
        ("Waga bed → tension:% mood:% bpm:% title:%"
            .format(t.round(0.01), ~waga[\mood], ~waga[\bpm], ~waga[\title])).postln;
    };

    // Pulse pattern reacts to tension
    ~pulseTask = Task({
        loop {
            var dens = 0.5 + (~waga[\tension] * 1.5);
            var f = 180 * (1 + ~waga[\tension]);
            if(~waga[\gate] == 1) {
                Synth(\wagaPulse, [\freq, f, \amp, 0.02 + (~waga[\tension] * 0.04)]);
            };
            (1.0 / dens).wait;
        }
    }).play(~clock);

    // OSC from Waga (Rust sends to sclang 57120)
    OSCdef(\wagaTension, { |msg| ~waga[\tension] = msg[1].asFloat; ~apply.value; }, '/waga/tension');
    OSCdef(\wagaBpm, { |msg| ~waga[\bpm] = msg[1].asFloat; ~apply.value; }, '/waga/bpm');
    OSCdef(\wagaMood, { |msg| ~waga[\mood] = msg[1].asString; }, '/waga/mood');
    OSCdef(\wagaStory, { |msg| ~waga[\story_open] = msg[1].asInteger; }, '/waga/story_open');
    OSCdef(\wagaDirty, { |msg| ~waga[\git_dirty] = msg[1].asInteger; }, '/waga/git_dirty');
    OSCdef(\wagaTitle, { |msg| ~waga[\title] = msg[1].asString; }, '/waga/title');
    OSCdef(\wagaGate, { |msg| ~waga[\gate] = msg[1].asInteger; ~apply.value; }, '/waga/gate');

    ~apply.value;
    "Waga HumanMusic bed ready — OSC on sclang port 57120".postln;
});
)
