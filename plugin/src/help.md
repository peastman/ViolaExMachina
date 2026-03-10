# Playing

Viola Ex Machina is a monophonic instrument: each instance plays only one note at a time.  To
create splits use multiple tracks, each with its own instance of the plugin.  If you press a
new note before releasing the previous one, it is played legato, smoothly blending between them.

# Articulations

An articulation is a style of playing the instrument to produce a particular sound.  The following
articulations are supported.

- **Arco**.  Long bow strokes with a gentle attack.  Key velocity controls the attack speed.  This
  articulation is particularly useful for slow, legato passages.
- **Marcato**.  Similar to arco, but with an accent at the beginning of each note.  Key velocity
  controls the strength of the accent.  This articulation works especially well for fast passages
  and for short, staccato notes.
- **Spiccato**.  Very short notes created by bouncing the bow off the string.  Key velocity controls
  the volume of each note.
- **Pizzicato**.  The player plucks the string with their finger.  Key velocity controls
  the volume of each note.
- **Tremolo**.  The player moves the bow back and forth as quickly as possible to create a pulsing
  sound. 

# Parameters

There are several additional parameters you can automate in a DAW to control the performance.

- **Dynamics**.  How loud to play.  This is not simply a volume control.  Instruments sound different
  depending on how loudly they are playing.
- **Vibrato**.  The amount of vibrato to add to the sound.
- **Bow Position**.  The position of the bow along the string.  Low values correspond to *sul
  ponticello*, which has a harsh, intense sound.  High values correspond to *sul tasto*, which has
  a mellow sound.  Values near the middle of the range correspond to normal bowing.
- **Bow Noise**.  The amount of noise from the bow scraping the string.
- **Release Rate**.  How quickly the sound stops at the end of a note.
- **Stereo Width**.  How widely the instruments in the ensemble are spread out in space.
- **Time Spread**.  The amount of delay between instruments in the ensemble.
- **Harmonics**.  The player fingers each note as usual, but uses a second finger to lightly touch
  the string 1/4 of the way along its length.  This damps all frequencies that do not have a node
  at that position, creating a thin sound two octaves higher than usual.
- **Con Sordino**.  A concert mute is placed on the bridge, altering the tone color and making the
  sound slightly quieter.