package it.tremendouspreparator.app;

import android.Manifest;
import android.bluetooth.BluetoothAdapter;
import android.bluetooth.BluetoothDevice;
import android.bluetooth.BluetoothManager;
import android.bluetooth.BluetoothServerSocket;
import android.bluetooth.BluetoothSocket;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.content.pm.PackageManager;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.util.UUID;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/**
 * Collegamento Bluetooth «classico» (RFCOMM) tra due telefoni vicini, anche senza Internet.
 * Trasporta gli stessi messaggi firmati e cifrati che andrebbero ai relay (vedi
 * crates/prep-app/src/app.rs): per l'app è un relay con un solo altro dispositivo.
 * Gli eventi arrivano a Rust con nativeBt: 0 dispositivo trovato ("nome\nindirizzo"),
 * 1 collegato (nome), 2 dati (un messaggio), 3 chiuso (motivo), 4 avviso.
 */
public final class BtLink {
    static final UUID SERVICE = UUID.fromString("6f1b3c2e-8d4a-4f51-9b7e-1d2c3a4b5c6d");
    private static final String[] PERMS = {
        Manifest.permission.BLUETOOTH_CONNECT, Manifest.permission.BLUETOOTH_SCAN, Manifest.permission.BLUETOOTH_ADVERTISE
    };

    static native void nativeBt(int kind, String text, byte[] data);

    static {
        // la libreria Rust è caricata da NativeActivity, ma così Java ne trova anche le funzioni JNI
        try {
            System.loadLibrary("prepapp");
        } catch (Throwable t) {
            // già disponibile o assente: gli eventi verranno ignorati
        }
    }

    private final MainActivity act;
    private final ExecutorService tx = Executors.newSingleThreadExecutor();
    private BluetoothServerSocket server;
    private BluetoothSocket sock;
    private DataOutputStream out;
    private boolean receiverOn;

    private final BroadcastReceiver found = new BroadcastReceiver() {
        @Override
        public void onReceive(Context c, Intent i) {
            if (BluetoothDevice.ACTION_FOUND.equals(i.getAction())) {
                BluetoothDevice d = i.getParcelableExtra(BluetoothDevice.EXTRA_DEVICE, BluetoothDevice.class);
                if (d != null) report(d);
            }
        }
    };

    BtLink(MainActivity act) {
        this.act = act;
    }

    private static void event(int kind, String text, byte[] data) {
        try {
            nativeBt(kind, text == null ? "" : text, data == null ? new byte[0] : data);
        } catch (Throwable t) {
            // libreria nativa non pronta
        }
    }

    private BluetoothAdapter adapter() {
        BluetoothManager m = act.getSystemService(BluetoothManager.class);
        return m == null ? null : m.getAdapter();
    }

    /** Bluetooth presente, acceso e con i permessi; altrimenti avvisa (e chiede i permessi). */
    private boolean ready() {
        BluetoothAdapter a = adapter();
        if (a == null) {
            event(4, "Questo dispositivo non ha il Bluetooth", null);
            return false;
        }
        for (String p : PERMS) {
            if (act.checkSelfPermission(p) != PackageManager.PERMISSION_GRANTED) {
                act.runOnUiThread(() -> act.requestPermissions(PERMS, 7));
                event(4, "Concedi i permessi «Dispositivi nelle vicinanze» e riprova", null);
                return false;
            }
        }
        if (!a.isEnabled()) {
            act.runOnUiThread(() -> {
                try {
                    act.startActivity(new Intent(BluetoothAdapter.ACTION_REQUEST_ENABLE));
                } catch (Exception e) {
                    // ignorato
                }
            });
            event(4, "Accendi il Bluetooth e riprova", null);
            return false;
        }
        return true;
    }

    private void report(BluetoothDevice d) {
        try {
            String name = d.getName();
            event(0, (name == null ? "Dispositivo" : name) + "\n" + d.getAddress(), null);
        } catch (SecurityException e) {
            // permesso revocato
        }
    }

    /** Si rende visibile e aspetta che l'altro telefono si colleghi. */
    void listen() {
        if (!ready()) return;
        act.runOnUiThread(() -> {
            Intent i = new Intent(BluetoothAdapter.ACTION_REQUEST_DISCOVERABLE);
            i.putExtra(BluetoothAdapter.EXTRA_DISCOVERABLE_DURATION, 180);
            try {
                act.startActivity(i);
            } catch (Exception e) {
                // ignorato
            }
        });
        new Thread(() -> {
            try {
                closeServer();
                server = adapter().listenUsingInsecureRfcommWithServiceRecord("TremendousPreparator", SERVICE);
                event(4, "Visibile: sull'altro telefono tocca «Cerca» e scegli questo", null);
                BluetoothSocket s = server.accept();
                closeServer();
                attach(s);
            } catch (IOException | SecurityException e) {
                event(3, "attesa interrotta", null);
            }
        }).start();
    }

    /** Elenca i dispositivi già associati e cerca quelli vicini. */
    void scan() {
        if (!ready()) return;
        try {
            BluetoothAdapter a = adapter();
            for (BluetoothDevice d : a.getBondedDevices()) report(d);
            if (!receiverOn) {
                act.registerReceiver(found, new IntentFilter(BluetoothDevice.ACTION_FOUND), Context.RECEIVER_EXPORTED);
                receiverOn = true;
            }
            a.cancelDiscovery();
            a.startDiscovery();
            event(4, "Ricerca dei dispositivi vicini…", null);
        } catch (SecurityException e) {
            event(4, "Permessi Bluetooth mancanti", null);
        }
    }

    void connect(String address) {
        if (!ready()) return;
        new Thread(() -> {
            try {
                BluetoothAdapter a = adapter();
                a.cancelDiscovery();
                BluetoothSocket s = a.getRemoteDevice(address).createInsecureRfcommSocketToServiceRecord(SERVICE);
                s.connect();
                attach(s);
            } catch (IOException | SecurityException | IllegalArgumentException e) {
                event(3, "collegamento non riuscito: sull'altro telefono tocca prima «Rendimi visibile»", null);
            }
        }).start();
    }

    private void attach(BluetoothSocket s) throws IOException {
        close();
        sock = s;
        out = new DataOutputStream(s.getOutputStream());
        String name = "telefono vicino";
        try {
            if (s.getRemoteDevice().getName() != null) name = s.getRemoteDevice().getName();
        } catch (SecurityException e) {
            // nome non disponibile
        }
        event(1, name, null);
        final BluetoothSocket mine = s;
        new Thread(() -> {
            try (DataInputStream in = new DataInputStream(mine.getInputStream())) {
                while (true) {
                    int n = in.readInt();
                    if (n <= 0 || n > (4 << 20)) break;
                    byte[] b = new byte[n];
                    in.readFully(b);
                    event(2, "", b);
                }
            } catch (IOException e) {
                // chiuso
            }
            if (sock == mine) {
                close();
                event(3, "collegamento Bluetooth chiuso", null);
            }
        }).start();
    }

    /** Manda un messaggio (in coda, fuori dal thread dell'interfaccia). */
    void send(byte[] data) {
        final DataOutputStream o = out;
        if (o == null) return;
        tx.execute(() -> {
            try {
                synchronized (o) {
                    o.writeInt(data.length);
                    o.write(data);
                    o.flush();
                }
            } catch (IOException e) {
                close();
                event(3, "collegamento Bluetooth interrotto", null);
            }
        });
    }

    private void closeServer() {
        try {
            if (server != null) server.close();
        } catch (IOException e) {
            // ignorato
        }
        server = null;
    }

    void close() {
        closeServer();
        try {
            if (sock != null) sock.close();
        } catch (IOException e) {
            // ignorato
        }
        sock = null;
        out = null;
    }
}
