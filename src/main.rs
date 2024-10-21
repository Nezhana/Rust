use eframe::egui::{self, CentralPanel, Context, TextEdit, Window};
use eframe::App;

struct CalculatorApp {
    input: String,
    result: String,
    memory: Vec<String>,
}

impl CalculatorApp {
    fn new() -> Self {
        Self {
            input: String::new(),
            result: String::new(),
            memory: Vec::new(),
        }
    }

    fn calculate(&mut self) {
        let trimmed_input = self.input.trim();
        let parsed_result = meval::eval_str(trimmed_input);

        match parsed_result {
            Ok(value) => {
                self.result = value.to_string();
                self.memory.push(format!("{} = {}", trimmed_input, value));
                if self.memory.len() > 3 {
                    self.memory.remove(0);
                }
            }
            Err(_) => {
                self.result = "Error: Invalid Input".to_string();
            }
        }
    }
}

impl App for CalculatorApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        CentralPanel::default().show(ctx, |ui| {
            ui.label("Simple Calculator");

            ui.horizontal(|ui| {
                ui.label("Expression:");
                ui.add(TextEdit::singleline(&mut self.input));
            });

            if ui.button("Calculate").clicked() {
                self.calculate();
            }

            ui.label(format!("Result: {}", self.result));

            Window::new("Memory").show(ctx, |ui| {
                for entry in self.memory.iter().rev() {
                    ui.label(entry);
                }
            });
        });
    }
}


fn main() {
    let options = eframe::NativeOptions::default();
    eframe::run_native("Simple Calculator", options, Box::new(|_cc| {
        Ok(Box::new(CalculatorApp::new()))
    }));
}
