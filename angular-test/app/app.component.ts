import { Component } from '@angular/core';
import { CommonModule } from '@angular/common';

@Component({
    selector: 'app-root',
    standalone: true,
    imports: [CommonModule],
    templateUrl: './app.component.html',
})
export class AppComponent {
    title = 'Hello World';
    features = [
        {
            name: '⚡ Angular 20',
            description: 'Modern Angular with standalone components'
        },
        {
            name: '🎨 Tailwind CSS',
            description: 'Utility-first CSS framework for rapid UI development'
        },
        {
            name: '🏗️ Bazel',
            description: 'Fast, scalable, and multi-language build system'
        }
    ];
}
