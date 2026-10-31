// patch: comfy/ldm/flux3/model.py — autograd-safe cat (already applied in v0.8)
// See flux3_train_cat.patch

// patch: comfy_extras/nodes_train.py — _load_existing_lora must tolerate
// LoRA names that do not carry a "<n>_steps" segment (our renamed files).
// Without it, resuming from "my_style.safetensors" raises ValueError.
