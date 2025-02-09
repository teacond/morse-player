use std::time::Duration;

use morse_player;

#[tokio::main]
async fn main() {
    let mut audio_player = morse_player::AudioPlayer::new();
    audio_player.set_speed(100.0);
    audio_player.set_text(&vec!['H', 'E', 'L', 'L', 'O', ' ', 'W', 'O', 'R', 'L', 'D'].to_vec());
    audio_player.set_text_type(morse_player::TextType::Digits);
    audio_player.set_text_additions(morse_player::TextAdditions::None);
    audio_player.play().await;
    tokio::time::sleep(Duration::from_millis(500)).await;
}