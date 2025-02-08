use std::{collections::HashMap, sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex}, thread, time::Duration};
use rodio::{OutputStream, OutputStreamHandle, Sink};
use ndarray::Array1;
use std::f32::consts::PI;
use tokio::{self, time::sleep};

const SAMPLE_RATE: u32 = 48000;
const LETTERS_DURATION: f32 = 0.05;
const DIGITS_DURATION: f32 = 0.034;
const MIXED_DURATION: f32 = 0.042;
const HARMONICS_COUNT: u32 = 20;
const FADE_IN: f32 = 0.0004;
const FADE_OUT: f32 = 0.0002;
const START_TEXT: [char; 35] = ['*', '.', '*', '.', '*', '.', '*', '-', '$',
                                '.', '*', '.', '*', '.', '*', '-', '$',
                                '.', '*', '.', '*', '.', '*', '-', '/',
                                '-', '*', '.', '*', '.', '*', '.', '*', '-', '/'];
const END_TEXT: [char; 10] = ['/', '.', '*', '-', '*', '.', '*', '-', '*', '.'];
const SINK_BUFFER_SIZE: u32 = 3;

pub type PlayingStartedCallback = Arc<dyn Fn() + 'static>;
pub type PlayingEndedCallback = Arc<dyn Fn() + 'static>;

pub struct AudioPlayer {
    text: Vec<char>,
    text_type: i32,
    speed: f32,
    modification: i32,
    min_speed: f32,
    max_speed: f32,
    modification_len: i32,
    _stream: Arc<Mutex<OutputStream>>,
    _stream_handle: Arc<Mutex<OutputStreamHandle>>,
    sink: Arc<Mutex<Sink>>,
    stop_flag: Arc<AtomicBool>,
    playing_started_callback: Option<PlayingStartedCallback>,
    playing_ended_callback: Option<PlayingEndedCallback>,
    actions_length: Arc<Mutex<HashMap<char, (i32, i32)>>>,
    additions: i32,
    wave_type: i32,
    frequency: i32,
}

impl AudioPlayer {
    pub fn new() -> AudioPlayer {
        let (stream, stream_handle) = OutputStream::try_default().unwrap();
        let sink = Sink::try_new(&stream_handle).unwrap();
        sink.set_volume(0.5);
        let mut m = HashMap::new();
        m.insert('.', (0, 1));
        m.insert('-', (0, 3));
        m.insert('*', (1, 1));
        m.insert('$', (1, 3));
        m.insert('/', (1, 7));
        m.insert('|', (2, 0));

        AudioPlayer {text: Vec::<char>::new(), 
            text_type: 0, 
            speed: 100.0,
            modification: 0, 
            min_speed: 100.0, 
            max_speed: 110.0, 
            modification_len: 10,
            _stream: Arc::new(Mutex::new(stream)), 
            _stream_handle: Arc::new(Mutex::new(stream_handle)),
            sink: Arc::new(Mutex::new(sink)),
            stop_flag: Arc::new(AtomicBool::new(false)),
            playing_started_callback: None,
            playing_ended_callback: None,
            actions_length: Arc::new(Mutex::new(m)),
            additions: 1,
            wave_type: 0,
            frequency: 750
        }
    }

    pub fn get_text_duration(&self) -> f32 {
        let (speed_pattern, text_preview) = gen_audio_prev_vec(&self.text, 0, self.min_speed, self.max_speed, self.modification, self.modification_len);
        let (text_time, _) = get_time_and_timings(&text_preview, self.text_type, self.speed, Some(&speed_pattern), &self.actions_length.lock().unwrap());
        return text_time
    }

    pub fn get_start_part_duration(&self) -> f32 {
        let mut speed: f32 = self.speed;
        if self.modification == 1 || self.modification == 3 {
            speed = self.min_speed;
        } else if self.modification == 2 {
            speed = self.max_speed;
        }
        let (text_time, _) = get_time_and_timings(&START_TEXT.to_vec(), self.text_type, speed, None, &self.actions_length.lock().unwrap());
        return text_time
    }

    pub fn get_char_timings(&self) -> Vec<Duration> {
        let (speed_pattern, text_preview) = gen_audio_prev_vec(&self.text, 0, self.min_speed, self.max_speed, self.modification, self.modification_len);
        let (_, time_pattern) = get_time_and_timings(&text_preview, self.text_type, self.speed, Some(&speed_pattern), &self.actions_length.lock().unwrap());
        return time_pattern
    }

    pub fn set_text(&mut self, text: &Vec<char>) {
        self.text = text.to_vec();
    }

    pub fn set_text_type(&mut self, text_type: i32) {
        self.text_type = text_type;
    }

    pub fn set_speed(&mut self, speed: f32) {
        self.speed = speed;
    }
    
    pub fn set_min_speed(&mut self, min_speed: f32) {
        self.min_speed = min_speed;
    }

    pub fn set_max_speed(&mut self, max_speed: f32) {
        self.max_speed = max_speed;
    }

    pub fn set_modification(&mut self, modification: i32) {
        self.modification = modification;
    }
    pub async fn play(&self) {
        let local = tokio::task::LocalSet::new();
        let end_notification = Arc::new(tokio::sync::Notify::new());
    
        let text = self.text.clone();
        let text_type = self.text_type.clone();
        let mut speed = self.speed;
        let min_speed = self.min_speed;
        let max_speed = self.max_speed;
        let modification = self.modification;
        let sink = self.sink.clone();
        let stop_flag = self.stop_flag.clone();
        let start_callback = self.playing_started_callback.clone();
        let end_callback = self.playing_ended_callback.clone();
        let actions_length = self.actions_length.lock().unwrap().clone();
        let modification_len = self.modification_len;
        let additions = self.additions;
        let frequency = self.frequency;
        let wave_type = self.wave_type;
    
        stop_flag.store(false, Ordering::SeqCst);
        sink.lock().unwrap().play();
    
        if modification == 1 || modification == 3 {
            speed = min_speed;
        } else if modification == 2 {
            speed = max_speed;
        }
    
        let end_notification_ref = Arc::clone(&end_notification);
        let end_notification_ref2 = Arc::clone(&end_notification);
        let start_part_duration = self.get_start_part_duration();
    
        thread::spawn(move || {
            let unlocked_sink = sink.lock().unwrap();
            let (mode_speed_pattern, text_preview) = gen_audio_prev_vec(
                &text,
                additions,
                min_speed,
                max_speed,
                modification,
                modification_len,
            );
            play_audio(
                &text_preview,
                text_type,
                speed,
                &unlocked_sink,
                &stop_flag,
                &mode_speed_pattern,
                &actions_length,
                frequency,
                wave_type,
            );
            end_notification.notify_waiters();
        });
    
        if additions == 1 {
            local.spawn_local(async move {
                if let Some(callback) = start_callback {
                    tokio::select! {
                        _ = end_notification_ref.notified() => {

                        }
                        _ = sleep(Duration::from_millis((start_part_duration * 1000.0) as u64)) => {
                            callback();
                        }
                    }
                }
            });
        }
    
        local.spawn_local(async move {
            end_notification_ref2.notified().await;
            if let Some(callback) = end_callback {
                callback();
            }
        });
    
        local.await;
    }
    
    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        self.sink.lock().unwrap().clear();
    }

    pub fn connect_playing_started_callback<F>(&mut self, callback: F)
    where
        F: Fn() + 'static,
    {
        self.playing_started_callback = Some(Arc::new(callback));
    }

    pub fn connect_playing_ended_callback<F>(&mut self, callback: F)
    where
        F: Fn() + 'static,
    {
        self.playing_ended_callback = Some(Arc::new(callback));
    }

    pub fn set_delay(&self, delay: i32) {
        self.actions_length.lock().unwrap().insert('$', (1, delay));
        self.actions_length.lock().unwrap().insert('/', (1, (delay as f64 * 2.33) as i32));    
    }

    pub fn set_modification_length(&mut self, length: i32) {
        self.modification_len = length;
    }

    pub fn set_frequency(&mut self, frequency: i32) {
        self.frequency = frequency;
    }
    
    pub fn set_wave_type(&mut self, wave_type: i32) {
        self.wave_type = wave_type;
    }

    pub fn set_volume(&mut self, volume: i32) {
        self.sink.lock().unwrap().set_volume(volume as f32 / 100.0);
    }
}

/*
    PAUSES DESCRIPTION:
        * - Pause beetween dots or dashes
        $ - Pause beetween characters
        / - Pause beetween words

*/

fn apply_hann_window(samples: &mut Array1<f32>, fade_in_samples: usize, fade_out_samples: usize) {
    let hann_in = Array1::linspace(0.0, PI, fade_in_samples)
        .mapv(|x| 0.5 * (1.0 - f32::cos(x as f32)));

    let hann_out = Array1::linspace(PI, 0.0, fade_out_samples)
        .mapv(|x| 0.5 * (1.0 - f32::cos(x as f32)));

    for i in 0..fade_in_samples {
        samples[i] *= hann_in[i];
    }

    for i in 0..fade_out_samples {
        let len = samples.len();
        samples[len - fade_out_samples + i] *= hann_out[i];
    }
}

fn get_wave(wave_type: i32, frequency: i32, speed_to_use: f32, duration_multiplier: i32) -> Array1::<f32> {
    let fade_in_samples = (SAMPLE_RATE as f32 * FADE_IN) as usize;
    let fade_out_samples = (SAMPLE_RATE as f32 * FADE_OUT) as usize;
    let samples_count_in_dot = SAMPLE_RATE as f32 * speed_to_use;
    let samples_wave_count = samples_count_in_dot * duration_multiplier as f32;
    let t_wave = Array1::linspace(0.0, speed_to_use * duration_multiplier as f32, samples_wave_count as usize);
    let mut wave = match wave_type {
        0 => { // square wave
            let mut wave = Array1::zeros(t_wave.len());
            for harmonic in 0..HARMONICS_COUNT {
                let harmonic_frequency = (frequency * (2 * harmonic as i32 + 1)) as f32;
                let harmonic_wave = (2.0 * PI * harmonic_frequency * &t_wave).mapv(f32::sin);
                wave = wave + harmonic_wave / (2 * harmonic + 1) as f32;
            }
            wave
        }
        1 => { // sine wave
            (2.0 * PI * frequency as f32 * t_wave).mapv(f32::sin)
        }
        2 => { // triangle wave
            let mut wave = Array1::zeros(t_wave.len());
            for harmonic in 0..HARMONICS_COUNT {
                let harmonic_frequency = (frequency * (2 * harmonic as i32 + 1)) as f32;
                let harmonic_wave = (2.0 * PI * harmonic_frequency * &t_wave).mapv(f32::sin);
                let sign = if harmonic % 2 == 0 { 1.0 } else { -1.0 };
                wave = wave + sign * harmonic_wave / ((2 * harmonic + 1).pow(2)) as f32;
            }
            wave
        }
        3 => { // sawtooth wave
            let mut wave = Array1::zeros(t_wave.len());
            for harmonic in 1..HARMONICS_COUNT {
                let harmonic_frequency = (frequency * harmonic as i32) as f32;
                let harmonic_wave = (2.0 * PI * harmonic_frequency * &t_wave).mapv(f32::sin);
                wave = wave + harmonic_wave / harmonic as f32;
            }
            wave
        }
        _ => panic!("Invalid wave type"),
    };

    // Wave normalization
    let max_amplitude = wave.iter().cloned().fold(f32::MIN, f32::max).abs();
    if max_amplitude > 0.0 {
        wave = wave / max_amplitude;
    }

    apply_hann_window(&mut wave, fade_in_samples, fade_out_samples);

    wave
}

fn get_silence(speed_to_use: f32, duration_multiplier: i32) -> Vec<f32> {
    let samples_count_in_dot = SAMPLE_RATE as f32 * speed_to_use;
    let samples_wave_count = samples_count_in_dot * duration_multiplier as f32;
    let silence: Vec<f32> = vec![0.0; samples_wave_count as usize];
    silence
}

fn play_audio(text: &Vec<char>, text_type: i32, speed: f32, sink: &Sink, stop_flag: &Arc<AtomicBool>, 
    speed_pattern: &Vec<f32>, actions_length: &HashMap<char, (i32, i32)>, frequency: i32, wave_type: i32) {
    let mut sound_signal = Vec::<f32>::new();
    let mut speed_to_use = get_speed_from_text_type(text_type, speed);
    let mut char_now = 0;
    let mut short_wave = get_wave(wave_type, frequency, speed_to_use, actions_length.get(&'.').unwrap().1);
    let mut long_wave = get_wave(wave_type, frequency, speed_to_use, actions_length.get(&'-').unwrap().1);
    let mut short_silence = get_silence(speed_to_use, actions_length.get(&'*').unwrap().1);
    let mut medium_silence = get_silence(speed_to_use, actions_length.get(&'$').unwrap().1);
    let mut long_silence = get_silence(speed_to_use, actions_length.get(&'/').unwrap().1);

    for (i, element) in text.iter().enumerate() {
        let action_description = actions_length.get(&element);
        let action: i32 = action_description.unwrap().0;

        if action == 0 {
            if element == &'.' {
                sound_signal.extend(short_wave.clone());
            }
            else {
                sound_signal.extend(long_wave.clone());
            }
        }
        else if action == 1 {
            if element == &'*' {
                sound_signal.extend(short_silence.clone());
            }
            else if element == &'$' {
                sound_signal.extend(medium_silence.clone());
            }
            else {
                sound_signal.extend(long_silence.clone());
            }
        }
        else if action == 2 {
            speed_to_use = get_speed_from_text_type(text_type, speed_pattern[char_now]);
            short_wave = get_wave(wave_type, frequency, speed_to_use, actions_length.get(&'.').unwrap().1);
            long_wave = get_wave(wave_type, frequency, speed_to_use, actions_length.get(&'-').unwrap().1);
            short_silence = get_silence(speed_to_use, actions_length.get(&'*').unwrap().1);
            medium_silence = get_silence(speed_to_use, actions_length.get(&'$').unwrap().1);
            long_silence = get_silence(speed_to_use, actions_length.get(&'/').unwrap().1); 
            char_now += 1;
        }

        if *element == '/' || i+1 == text.len() {
            loop {
                if sink.len() <= SINK_BUFFER_SIZE as usize {
                    break;
                }
                if stop_flag.load(Ordering::SeqCst) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            sink.append(rodio::buffer::SamplesBuffer::new(1, SAMPLE_RATE, sound_signal.to_vec()));
            sound_signal.clear();
        }
    }

    while sink.len() != 0 {
        if stop_flag.load(Ordering::SeqCst) {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/* 

    ADDITIONS:
        0   without additions
        1   VVV= and end part
        2   competitions part

*/

fn gen_audio_prev_vec(text: &Vec<char>, additions: i32, min_speed: f32, max_speed: f32, modification: i32, modification_len: i32) -> (Vec<f32>, Vec<char>) {
    let morse: HashMap<char, &str> = [
        ('A', ".-"), ('B', "-..."), ('C', "-.-."), ('D', "-.."), ('E', "."),
        ('F', "..-."), ('G', "--."), ('H', "...."), ('I', ".."), ('J', ".---"),
        ('K', "-.-"), ('L', ".-.."), ('M', "--"), ('N', "-."), ('O', "---"),
        ('P', ".--."), ('Q', "--.-"), ('R', ".-."), ('S', "..."), ('T', "-"),
        ('U', "..-"), ('V', "...-"), ('W', ".--"), ('X', "-..-"), ('Y', "-.--"),
        ('Z', "--.."), ('0', "-----"), ('1', ".----"), ('2', "..---"), ('3', "...--"),
        ('4', "....-"), ('5', "....."), ('6', "-...."), ('7', "--..."), ('8', "---.."),
        ('9', "----."), ('.', ".-.-.-"), (',', "--..--"), ('/', "-..-."), ('?', "..--.."),
        ('=', "-...-")].iter().cloned().collect();
    let mut audio_vec = Vec::<char>::new();
    let mut speed_pattern = Vec::<f32>::new();
    let speed_difference = max_speed - min_speed;
    let modification_len = modification_len * 5;
    let mut char_now: i32 = 0;
    if additions == 1 {
        for element in START_TEXT {
            audio_vec.push(element);
        }
    }
    for (i, element) in text.iter().enumerate() {
        if *element != ' ' && modification > 0 {
            let speed_on_char: f32 = match modification {
                1 => {
                    let speed_on_char = speed_difference / (modification_len - 1) as f32 * char_now as f32 + min_speed;
                    speed_on_char
                }
                2 => {
                    let speed_on_char = max_speed - (speed_difference / (modification_len - 1) as f32 * char_now as f32);
                    speed_on_char
                }
                3 => {
                    let speed_on_char: f32;
                    if char_now < modification_len / 2 {
                        speed_on_char = speed_difference / ((modification_len / 2) - 1) as f32 * char_now as f32 + min_speed;
                    }
                    else {
                        speed_on_char = max_speed - (speed_difference / ((modification_len / 2) - 1) as f32 * (char_now - modification_len / 2) as f32);
                    }
                    speed_on_char
                }
                _ => {
                    panic!("Invalid Modification type");
                },
            };

            speed_pattern.push(speed_on_char);

            char_now += 1;
            if char_now == modification_len {
                char_now = 0;
            }

            audio_vec.push('|'); // char, that inform play function to recalculate speed
        }
        if let Some(morse_code) = morse.get(&element) {
            for (n, morse_char) in morse_code.chars().enumerate() {
                audio_vec.push(morse_char);
                if n+1 != morse_code.len() {
                    audio_vec.push('*');
                }
            }
        }

        if *element != ' ' && i != text.len() - 1 {
            audio_vec.push('$');
        }
        else if *element == ' ' {
            let audio_vec_len = audio_vec.len();
            if char_now == 0 && modification > 0 { // if enabled modification, make latest silence long
                speed_pattern.push(min_speed);
                audio_vec[audio_vec_len - 1] = '|';
                audio_vec.push('/');
            }
            else {
                audio_vec[audio_vec_len - 1] = '/';
            }
        }
    }

    if additions == 1 { 
        for element in END_TEXT {
            audio_vec.push(element);
        }
    }

    return (speed_pattern, audio_vec);
}

fn get_speed_from_text_type(text_type: i32, speed: f32) -> f32 { // calculating absolute speed of text
    let speed_to_use;
    if text_type == 0 {
        speed_to_use = LETTERS_DURATION * 100.0 / speed;
    }
    else if text_type == 1 {
        speed_to_use = DIGITS_DURATION * 100.0 / speed;
    }
    else {
        speed_to_use = MIXED_DURATION * 100.0 / speed;
    }
    speed_to_use
}

fn get_time_and_timings(audio_prev_vec: &Vec<char>, text_type: i32, speed: f32, speed_pattern: Option<&Vec<f32>>, actions_length: &HashMap<char, (i32, i32)>) -> (f32, Vec<Duration>) {
    let mut time_pattern_vec = Vec::<Duration>::new();
    let mut duration: f32 = 0.0;
    let mut char_now = 0;
    let mut speed_to_use: f32 = get_speed_from_text_type(text_type, speed);
    time_pattern_vec.push(Duration::from_millis(0));

    for element in audio_prev_vec {
        let action_discription = actions_length.get(&element);
        let duration_multiplier = action_discription.unwrap().1;
        duration += speed_to_use * duration_multiplier as f32;

        if action_discription.unwrap().0 == 2 {
            speed_to_use = get_speed_from_text_type(text_type, speed_pattern.unwrap()[char_now]);
            char_now += 1;
        }

        if *element == '$' || *element == '/' {
            time_pattern_vec.push(Duration::from_millis((duration * 1000.0) as u64));
        }
    }
    (duration, time_pattern_vec)
}