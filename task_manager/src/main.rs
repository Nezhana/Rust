use eframe::egui::{self, Button, CentralPanel, Context, TextEdit, Window};
use eframe::App;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Serialize, Deserialize, Clone)]
struct Task {
    description: String,
    done: bool,
}

struct TaskManagerApp {
    tasks: Vec<Task>,
    show_import_export: bool,
}

impl TaskManagerApp {
    fn new() -> Self {
        Self {
            tasks: Vec::new(),
            show_import_export: false,
        }
    }

    fn add_task(&mut self, description: String) {
        self.tasks.push(Task {
            description,
            done: false,
        });
    }

    fn delete_task(&mut self, index: usize) {
        if index < self.tasks.len() {
            self.tasks.remove(index);
        }
    }

    fn import_tasks(&mut self, filename: &str) -> Result<(), String> {
        let path = Path::new(filename);
        if path.exists() {
            let content = fs::read_to_string(filename).map_err(|e| e.to_string())?;
            self.tasks = serde_json::from_str(&content).map_err(|e| e.to_string())?;
            Ok(())
        } else {
            Err("File does not exist".to_string())
        }
    }

    fn export_tasks(&self, filename: &str) -> Result<(), String> {
        let content = serde_json::to_string(&self.tasks).map_err(|e| e.to_string())?;
        fs::write(filename, content).map_err(|e| e.to_string())
    }
}

impl App for TaskManagerApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        CentralPanel::default().show(ctx, |ui| {
            ui.heading("Task Manager");

            // add task
            ui.horizontal(|ui| {
                if ui.button("Add Task").clicked() {
                    self.add_task("New Task".to_string());
                }
            });

            // tasks changes
            let mut to_delete = Vec::new();
            for (i, task) in self.tasks.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    let label = if task.done {
                        format!("\u{2713} {}", task.description)
                    } else {
                        task.description.clone()
                    };

                    if ui.checkbox(&mut task.done, &label).changed() {
                        println!("Task {}: done = {}", task.description, task.done);
                    }

                    ui.text_edit_singleline(&mut task.description);

                    if ui.button("Delete").clicked() {
                        to_delete.push(i);
                    }
                });

            }

            for i in to_delete.into_iter().rev() {
                self.delete_task(i);
            }        

            // import/export btn
            if ui.button("Import/Export").clicked() {
                self.show_import_export = true;
            }
        });

        // import/export spare window
        if self.show_import_export {
            let mut show_import_export = self.show_import_export;
            Window::new("Import/Export")
                .open(&mut show_import_export)
                .show(ctx, |ui| {
                    if ui.button("Import from tasks.json").clicked() {
                        if let Err(err) = self.import_tasks("tasks.json") {
                            ui.label(format!("Import failed: {}", err));
                        } else {
                            ui.label("Import successful.");
                        }
                    }

                    if ui.button("Export to tasks.json").clicked() {
                        if let Err(err) = self.export_tasks("tasks.json") {
                            ui.label(format!("Export failed: {}", err));
                        } else {
                            ui.label("Export successful.");
                        }
                    }
                });

        }
    }
}

fn main() {
    let options = eframe::NativeOptions::default();
    eframe::run_native("Task Manager", options, Box::new(|_cc| {
        Ok(Box::new(TaskManagerApp::new()))
    }));
}