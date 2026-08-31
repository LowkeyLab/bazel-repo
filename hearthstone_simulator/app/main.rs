use hearthstone_simulator::Simulation;
use hearthstone_simulator_core::{Card, GameAction, PlayerConfig, PlayerId};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut simulation = Simulation::new([
        PlayerConfig::new("Player One", vec![Card::minion("Training Minion", 1, 1, 2)]),
        PlayerConfig::new("Player Two", Vec::new()),
    ]);
    let card = simulation.snapshot().players[0].hand[0];

    simulation.apply(GameAction::PlayCard {
        player: PlayerId::One,
        card,
        target: None,
        board_index: None,
        choice: None,
    })?;
    simulation.apply(GameAction::EndTurn {
        player: PlayerId::One,
    })?;

    println!("{:#?}", simulation.snapshot());
    Ok(())
}
