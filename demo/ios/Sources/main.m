// The app's entry: hands the main thread to the Rust staticlib, which runs eframe's event
// loop and does not return while the app runs.
extern void corundum_demo_main(void);

int main(int argc, char *argv[]) {
    corundum_demo_main();
    return 0;
}
